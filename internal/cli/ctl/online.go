package ctl

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"text/tabwriter"
	"time"

	parser "github.com/themadorg/madmail/framework/cfgparser"
	maddycli "github.com/themadorg/madmail/internal/cli"
	mdb "github.com/themadorg/madmail/internal/db"
	"github.com/themadorg/madmail/internal/servertracker"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "status",
		Usage: "Show server status: connections, users, uptime",
		Description: `Shows a comprehensive server status including active connections,
registered user count, boot time, uptime, and email server tracking.

The command reads the maddy configuration file to determine which ports
the services are listening on, then uses 'ss' to count established
connections. It also queries the database for user count.

Services tracked:
  - IMAP (default: 143, 993)
  - Chatmail ALPN-multiplexed IMAP (default: 443)
  - TURN relay (default: 3478, both TCP and UDP)
  - Shadowsocks proxy (default: 8388)

Examples:
  maddy status
  maddy status --details
`,
		Flags: []cli.Flag{
			&cli.BoolFlag{
				Name:    "details",
				Aliases: []string{"d"},
				Usage:   "Show per-port breakdown",
			},
		},
		Action: func(ctx *cli.Context) error {
			return statusAction(ctx)
		},
	})
}

// servicePortInfo holds information about a service listening port.
type servicePortInfo struct {
	Port    string
	Label   string
	Service string // "IMAP", "TURN", "Shadowsocks"
	Proto   string // "tcp" or "udp"
}

// parseResult holds the parsed configuration.
type parseResult struct {
	Ports      []servicePortInfo
	RuntimeDir string
	StateDir   string
	DBDriver   string
	DBDsn      string
}

// parseServicePorts reads the maddy config and extracts all service listening ports.
func parseServicePorts(cfgPath string) (parseResult, error) {
	cfgFile, err := os.Open(cfgPath)
	if err != nil {
		return parseResult{}, fmt.Errorf("failed to open config: %v", err)
	}
	defer cfgFile.Close()

	cfgNodes, err := parser.Read(cfgFile, cfgFile.Name())
	if err != nil {
		return parseResult{}, fmt.Errorf("failed to parse config: %v", err)
	}

	var ports []servicePortInfo
	runtimeDir := "/run/maddy"
	stateDir := "/var/lib/maddy"
	dbDriver := "sqlite3"
	dbDsn := "imapsql.db"

	// Regex to extract scheme and port from addresses like "tcp://0.0.0.0:143", "tls://0.0.0.0:993", "udp://0.0.0.0:3478"
	addrRe := regexp.MustCompile(`^(tcp|tls|udp)://[^:]+:(\d+)$`)

	for _, node := range cfgNodes {
		// Pick up runtime_dir if set
		if node.Name == "runtime_dir" && len(node.Args) > 0 {
			runtimeDir = node.Args[0]
		}
		if node.Name == "state_dir" && len(node.Args) > 0 {
			stateDir = node.Args[0]
		}
		switch node.Name {
		case "imap":
			// imap tls://0.0.0.0:993 tcp://0.0.0.0:143 { ... }
			for _, addr := range node.Args {
				matches := addrRe.FindStringSubmatch(addr)
				if matches == nil {
					continue
				}
				scheme := matches[1]
				port := matches[2]
				label := "IMAP"
				if scheme == "tls" {
					label = "IMAP TLS"
				}
				ports = append(ports, servicePortInfo{
					Port:    port,
					Label:   label,
					Service: "IMAP",
					Proto:   "tcp",
				})
			}

		case "chatmail":
			// chatmail tls://0.0.0.0:443 { alpn_imap imap; ss_addr "0.0.0.0:8388" ... }
			hasALPNIMAP := false
			ssAddr := ""
			for _, child := range node.Children {
				if child.Name == "alpn_imap" {
					hasALPNIMAP = true
				}
				if child.Name == "ss_addr" && len(child.Args) > 0 {
					ssAddr = strings.Trim(child.Args[0], "\"")
				}
			}
			if hasALPNIMAP {
				for _, addr := range node.Args {
					matches := addrRe.FindStringSubmatch(addr)
					if matches == nil {
						continue
					}
					port := matches[2]
					ports = append(ports, servicePortInfo{
						Port:    port,
						Label:   "ALPN (chatmail)",
						Service: "IMAP",
						Proto:   "tcp",
					})
				}
			}
			if ssAddr != "" {
				// ss_addr is "host:port"
				parts := strings.Split(ssAddr, ":")
				if len(parts) >= 2 {
					ssPort := parts[len(parts)-1]
					ports = append(ports, servicePortInfo{
						Port:    ssPort,
						Label:   "Shadowsocks",
						Service: "Shadowsocks",
						Proto:   "tcp",
					})
				}
			}

		case "turn":
			// turn udp://0.0.0.0:3478 tcp://0.0.0.0:3478 { ... }
			for _, addr := range node.Args {
				matches := addrRe.FindStringSubmatch(addr)
				if matches == nil {
					continue
				}
				scheme := matches[1]
				port := matches[2]
				proto := "tcp"
				label := "TURN TCP"
				if scheme == "udp" {
					proto = "udp"
					label = "TURN UDP"
				}
				ports = append(ports, servicePortInfo{
					Port:    port,
					Label:   label,
					Service: "TURN",
					Proto:   proto,
				})
			}
		case "storage.imapsql":
			// storage.imapsql local_mailboxes { driver sqlite3; dsn imapsql.db }
			for _, child := range node.Children {
				if child.Name == "driver" && len(child.Args) > 0 {
					dbDriver = child.Args[0]
				}
				if child.Name == "dsn" && len(child.Args) > 0 {
					dbDsn = strings.Join(child.Args, " ")
				}
			}
		}
	}

	// Fallback for IMAP if none found
	hasIMAP := false
	for _, p := range ports {
		if p.Service == "IMAP" {
			hasIMAP = true
			break
		}
	}
	if !hasIMAP {
		ports = append(ports,
			servicePortInfo{Port: "143", Label: "IMAP", Service: "IMAP", Proto: "tcp"},
			servicePortInfo{Port: "993", Label: "IMAP TLS", Service: "IMAP", Proto: "tcp"},
		)
	}

	// Deduplicate by port+proto (e.g. multiple chatmail blocks with same ss_addr)
	seen := make(map[string]bool)
	var deduped []servicePortInfo
	for _, p := range ports {
		key := p.Port + "/" + p.Proto
		if seen[key] {
			continue
		}
		seen[key] = true
		deduped = append(deduped, p)
	}

	return parseResult{Ports: deduped, RuntimeDir: runtimeDir, StateDir: stateDir, DBDriver: dbDriver, DBDsn: dbDsn}, nil
}

// connectionInfo stores details of a single connection.
type connectionInfo struct {
	LocalAddr  string
	RemoteAddr string
}

// getEstablishedConnections uses 'ss' to find established connections to the given port/proto.
func getEstablishedConnections(port, proto string) ([]connectionInfo, error) {
	var cmd *exec.Cmd
	if proto == "udp" {
		// For UDP, we look for connected/established UDP sockets
		// ss -unH sport = :PORT
		cmd = exec.Command("ss", "-unH", "sport", "= :"+port)
	} else {
		// ss -tnH state established sport = :PORT
		cmd = exec.Command("ss", "-tnH", "state", "established", "sport", "= :"+port)
	}

	output, err := cmd.Output()
	if err != nil {
		if proto == "tcp" {
			// Fallback to netstat for TCP
			cmd = exec.Command("netstat", "-tn")
			output, err = cmd.Output()
			if err != nil {
				return nil, fmt.Errorf("neither 'ss' nor 'netstat' available: %v", err)
			}
			return parseNetstatOutput(output, port)
		}
		return nil, fmt.Errorf("'ss' not available for UDP: %v", err)
	}

	return parseSSOutput(output)
}

// parseSSOutput parses the output of ss.
func parseSSOutput(output []byte) ([]connectionInfo, error) {
	var conns []connectionInfo
	scanner := bufio.NewScanner(strings.NewReader(string(output)))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		// ss output format: Recv-Q Send-Q Local Address:Port Peer Address:Port Process
		fields := strings.Fields(line)
		if len(fields) < 4 {
			continue
		}
		// For UDP, skip entries with peer "*:*" (unconnected)
		if fields[3] == "*:*" || fields[3] == "0.0.0.0:*" || fields[3] == "[::]:*" {
			continue
		}
		conns = append(conns, connectionInfo{
			LocalAddr:  fields[2],
			RemoteAddr: fields[3],
		})
	}
	return conns, nil
}

// getTurnRelayConnections counts active TURN relay allocations by finding
// maddy-owned UDP sockets on ephemeral ports (not the known TURN listening ports).
// TURN relay connections use dynamically allocated UDP ports, not port 3478.
func getTurnRelayConnections(knownPorts map[string]bool) (int, error) {
	// List all UDP sockets with process info
	cmd := exec.Command("ss", "-unap")
	output, err := cmd.Output()
	if err != nil {
		return 0, err
	}

	count := 0
	scanner := bufio.NewScanner(strings.NewReader(string(output)))
	for scanner.Scan() {
		line := scanner.Text()
		// Only count maddy-owned sockets
		if !strings.Contains(line, "\"maddy\"") {
			continue
		}
		fields := strings.Fields(line)
		// ss -unap output: State Recv-Q Send-Q LocalAddr:Port PeerAddr:Port Process
		//                  [0]   [1]    [2]    [3]            [4]           [5]
		if len(fields) < 5 {
			continue
		}
		localAddr := fields[3]
		peerAddr := fields[4]
		localPort := extractPort(localAddr)

		// Skip known listening ports (e.g. 3478)
		if knownPorts[localPort] {
			continue
		}

		// Skip entries with no peer that aren't relay sockets
		// Relay sockets have UNCONN state and peer *:* but on ephemeral ports
		if peerAddr == "*:*" || peerAddr == "0.0.0.0:*" || peerAddr == "[::]:*" {
			// Ephemeral UNCONN socket owned by maddy = TURN relay allocation
			count++
			continue
		}
		// Connected UDP socket (has a peer) — also a relay
		count++
	}
	return count, nil
}

// extractPort extracts the port from addr like "0.0.0.0:3478" or "*:3478" or "[::]:3478"
func extractPort(addr string) string {
	idx := strings.LastIndex(addr, ":")
	if idx == -1 {
		return addr
	}
	return addr[idx+1:]
}

// parseNetstatOutput parses netstat output filtering for the given port.
func parseNetstatOutput(output []byte, port string) ([]connectionInfo, error) {
	var conns []connectionInfo
	scanner := bufio.NewScanner(strings.NewReader(string(output)))
	for scanner.Scan() {
		line := scanner.Text()
		if !strings.Contains(line, "ESTABLISHED") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 5 {
			continue
		}
		localAddr := fields[3]
		// Check if the local port matches
		parts := strings.Split(localAddr, ":")
		if len(parts) < 2 {
			continue
		}
		localPort := parts[len(parts)-1]
		if localPort != port {
			continue
		}
		conns = append(conns, connectionInfo{
			LocalAddr:  localAddr,
			RemoteAddr: fields[4],
		})
	}
	return conns, nil
}

func statusAction(ctx *cli.Context) error {
	cfgPath := ctx.String("config")
	if cfgPath == "" {
		return cli.Exit("Error: config is required", 2)
	}

	parsed, err := parseServicePorts(cfgPath)
	if err != nil {
		return err
	}
	ports := parsed.Ports

	showDetails := ctx.Bool("details")

	type portResult struct {
		Info  servicePortInfo
		Conns []connectionInfo
	}
	var results []portResult

	// Track totals per service
	serviceTotals := make(map[string]int)
	serviceUniqueIPs := make(map[string]map[string]struct{})
	serviceOrder := []string{"IMAP", "TURN", "Shadowsocks"}

	// Collect known TURN ports to exclude from relay counting
	knownTurnPorts := make(map[string]bool)
	for _, p := range ports {
		if p.Service == "TURN" && p.Proto == "udp" {
			knownTurnPorts[p.Port] = true
		}
	}

	for _, p := range ports {
		conns, err := getEstablishedConnections(p.Port, p.Proto)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Warning: could not query %s port %s: %v\n", p.Proto, p.Port, err)
			continue
		}

		// For TURN UDP, also count relay allocations on ephemeral ports
		if p.Service == "TURN" && p.Proto == "udp" {
			relayCount, err := getTurnRelayConnections(knownTurnPorts)
			if err == nil && relayCount > 0 {
				// Each TURN allocation creates one relay socket
				for i := 0; i < relayCount; i++ {
					conns = append(conns, connectionInfo{
						LocalAddr:  "relay",
						RemoteAddr: "relay",
					})
				}
			}
		}

		serviceTotals[p.Service] += len(conns)
		if serviceUniqueIPs[p.Service] == nil {
			serviceUniqueIPs[p.Service] = make(map[string]struct{})
		}
		for _, c := range conns {
			if c.RemoteAddr != "relay" {
				serviceUniqueIPs[p.Service][extractIP(c.RemoteAddr)] = struct{}{}
			}
		}
		results = append(results, portResult{Info: p, Conns: conns})
	}

	// Print summary per service
	for _, svc := range serviceOrder {
		count, exists := serviceTotals[svc]
		if !exists {
			continue
		}
		ips := len(serviceUniqueIPs[svc])
		if svc == "TURN" {
			fmt.Printf("%-15s relays: %d\n", svc, count)
		} else {
			fmt.Printf("%-15s connections: %-6d unique IPs: %d\n", svc, count, ips)
		}
	}

	if showDetails && len(results) > 0 {
		fmt.Println()
		fmt.Println("Per-port breakdown:")
		w := tabwriter.NewWriter(os.Stdout, 0, 0, 3, ' ', 0)
		fmt.Fprintln(w, "PORT\tPROTO\tTYPE\tCONNECTIONS\tUNIQUE IPs")
		for _, r := range results {
			ips := make(map[string]struct{})
			for _, c := range r.Conns {
				if c.RemoteAddr != "relay" {
					ips[extractIP(c.RemoteAddr)] = struct{}{}
				}
			}
			if r.Info.Service == "TURN" && r.Info.Proto == "udp" {
				fmt.Fprintf(w, "%s\t%s\t%s\t%d relays\t-\n", r.Info.Port, r.Info.Proto, r.Info.Label, len(r.Conns))
			} else {
				fmt.Fprintf(w, "%s\t%s\t%s\t%d\t%d\n", r.Info.Port, r.Info.Proto, r.Info.Label, len(r.Conns), len(ips))
			}
		}
		w.Flush()
	}

	// Show user count from database
	userCount, err := getUserCount(parsed.DBDriver, parsed.DBDsn, parsed.StateDir)
	if err == nil {
		fmt.Printf("\nRegistered users:   %d\n", userCount)
	}

	// Show server status from the tracker file
	status, err := servertracker.ReadStatusFile(parsed.RuntimeDir)
	if err == nil {
		if status.BootTime > 0 {
			bootTime := time.Unix(status.BootTime, 0)
			uptime := time.Since(bootTime).Truncate(time.Second)
			fmt.Printf("Boot time:          %s (up %s)\n", bootTime.Format("2006-01-02 15:04:05"), formatUptime(uptime))
		}
		if status.UniqueConnIPs > 0 || status.UniqueDomains > 0 || status.UniqueIPServers > 0 {
			fmt.Println()
			fmt.Println("Email servers seen (since last restart):")
			fmt.Printf("  Connection IPs:   %d\n", status.UniqueConnIPs)
			fmt.Printf("  Domain servers:   %d\n", status.UniqueDomains)
			fmt.Printf("  IP servers:       %d\n", status.UniqueIPServers)
		}
	}

	return nil
}

// getUserCount queries the database for the number of registered users.
// Uses the project's GORM layer to support sqlite3, postgres, and mysql.
func getUserCount(driver, dsn, stateDir string) (int, error) {
	if driver == "" || dsn == "" {
		return 0, fmt.Errorf("database not configured")
	}

	// For sqlite3, resolve relative paths against stateDir
	if (driver == "sqlite3" || driver == "sqlite") && !filepath.IsAbs(dsn) {
		if stateDir == "" {
			stateDir = "/var/lib/maddy"
		}
		dsn = filepath.Join(stateDir, dsn)
	}

	db, err := mdb.New(driver, []string{dsn}, false)
	if err != nil {
		return 0, err
	}
	sqlDB, err := db.DB()
	if err == nil {
		defer sqlDB.Close()
	}

	var count int64
	if err := db.Table("users").Count(&count).Error; err != nil {
		return 0, err
	}
	return int(count), nil
}

// extractIP extracts the IP address from an addr:port or [ipv6]:port string.
func extractIP(addr string) string {
	// Handle IPv6: [::1]:port
	if strings.HasPrefix(addr, "[") {
		idx := strings.LastIndex(addr, "]:")
		if idx != -1 {
			return addr[1:idx]
		}
		return strings.Trim(addr, "[]")
	}

	// Handle IPv4: 1.2.3.4:port
	idx := strings.LastIndex(addr, ":")
	if idx != -1 {
		// Check if there's more than one colon (IPv6 without brackets)
		if strings.Count(addr, ":") > 1 {
			// IPv6 address without brackets - try to parse port
			// ss sometimes shows like "::ffff:1.2.3.4:port"
			lastColon := strings.LastIndex(addr, ":")
			possiblePort := addr[lastColon+1:]
			if _, err := strconv.Atoi(possiblePort); err == nil {
				return addr[:lastColon]
			}
			return addr
		}
		return addr[:idx]
	}
	return addr
}

// formatUptime formats a duration into a human-readable string like "2d 5h 30m 15s".
func formatUptime(d time.Duration) string {
	days := int(d.Hours()) / 24
	hours := int(d.Hours()) % 24
	minutes := int(d.Minutes()) % 60
	seconds := int(d.Seconds()) % 60

	if days > 0 {
		return fmt.Sprintf("%dd %dh %dm %ds", days, hours, minutes, seconds)
	}
	if hours > 0 {
		return fmt.Sprintf("%dh %dm %ds", hours, minutes, seconds)
	}
	if minutes > 0 {
		return fmt.Sprintf("%dm %ds", minutes, seconds)
	}
	return fmt.Sprintf("%ds", seconds)
}
