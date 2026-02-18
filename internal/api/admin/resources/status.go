package resources

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"syscall"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/servertracker"
)

// StatusResponse is the response body for /admin/status.
type StatusResponse struct {
	IMAP             *ServiceStatus `json:"imap,omitempty"`
	TURN             *TurnStatus    `json:"turn,omitempty"`
	Shadowsocks      *ServiceStatus `json:"shadowsocks,omitempty"`
	Users            *UsersStatus   `json:"users"`
	Uptime           *UptimeStatus  `json:"uptime"`
	EmailServers     *EmailServers  `json:"email_servers,omitempty"`
	SentMessages     int64          `json:"sent_messages"`
	OutboundMessages int64          `json:"outbound_messages"`
	ReceivedMessages int64          `json:"received_messages"`
}

type ServiceStatus struct {
	Connections int `json:"connections"`
	UniqueIPs   int `json:"unique_ips"`
}

type TurnStatus struct {
	Relays int `json:"relays"`
}

type UsersStatus struct {
	Registered int `json:"registered"`
}

type UptimeStatus struct {
	BootTime string `json:"boot_time"`
	Duration string `json:"duration"`
}

type EmailServers struct {
	ConnectionIPs int `json:"connection_ips"`
	DomainServers int `json:"domain_servers"`
	IPServers     int `json:"ip_servers"`
}

// StatusDeps are the dependencies needed by the status resource handler.
type StatusDeps struct {
	GetUserCount func() (int, error)
	GetSetting   func(string) (string, bool, error)
}

// StatusHandler creates a handler for /admin/status.
func StatusHandler(deps StatusDeps) func(method string, body json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if method != "GET" {
			return nil, 405, fmt.Errorf("method %s not allowed, use GET", method)
		}

		resp := StatusResponse{}

		// User count
		if deps.GetUserCount != nil {
			count, err := deps.GetUserCount()
			if err == nil {
				resp.Users = &UsersStatus{Registered: count}
			}
		}

		// Server tracker status (boot time + email servers)
		runtimeDir := config.RuntimeDirectory
		if runtimeDir == "" {
			runtimeDir = "/run/maddy"
		}
		status, err := servertracker.ReadStatusFile(runtimeDir)
		if err == nil {
			if status.BootTime > 0 {
				bootTime := time.Unix(status.BootTime, 0)
				uptime := time.Since(bootTime).Truncate(time.Second)
				resp.Uptime = &UptimeStatus{
					BootTime: bootTime.Format(time.RFC3339),
					Duration: formatDuration(uptime),
				}
			}
			if status.UniqueConnIPs > 0 || status.UniqueDomains > 0 || status.UniqueIPServers > 0 {
				resp.EmailServers = &EmailServers{
					ConnectionIPs: status.UniqueConnIPs,
					DomainServers: status.UniqueDomains,
					IPServers:     status.UniqueIPServers,
				}
			}
		}

		// Live connection counts via ss
		imapPort := "993"
		turnPort := "3478"
		ssPort := "8388"
		if deps.GetSetting != nil {
			if v, ok, err := deps.GetSetting(KeyIMAPPort); err == nil && ok && v != "" {
				imapPort = v
			}
			if v, ok, err := deps.GetSetting(KeyTurnPort); err == nil && ok && v != "" {
				turnPort = v
			}
			if v, ok, err := deps.GetSetting(KeySsPort); err == nil && ok && v != "" {
				ssPort = v
			}
		}

		// IMAP connections
		conns, ips := countTCPConnections(imapPort)
		resp.IMAP = &ServiceStatus{Connections: conns, UniqueIPs: ips}

		// Shadowsocks connections
		conns, ips = countTCPConnections(ssPort)
		resp.Shadowsocks = &ServiceStatus{Connections: conns, UniqueIPs: ips}

		// TURN relays
		relays := countTurnRelays(turnPort)
		resp.TURN = &TurnStatus{Relays: relays}

		// Message counters
		resp.SentMessages = module.GetSentMessages()
		resp.OutboundMessages = module.GetOutboundMessages()
		resp.ReceivedMessages = module.GetReceivedMessages()

		return resp, 200, nil
	}
}

// countTCPConnections uses ss to count established TCP connections on a port.
func countTCPConnections(port string) (connections int, uniqueIPs int) {
	cmd := exec.Command("ss", "-tnH", "state", "established", "sport", "= :"+port)
	output, err := cmd.Output()
	if err != nil {
		return 0, 0
	}
	ips := make(map[string]struct{})
	scanner := bufio.NewScanner(strings.NewReader(string(output)))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" {
			continue
		}
		connections++
		fields := strings.Fields(line)
		if len(fields) >= 4 {
			ip := extractIPFromAddr(fields[3])
			if ip != "" {
				ips[ip] = struct{}{}
			}
		}
	}
	return connections, len(ips)
}

// countTurnRelays counts active TURN relay allocations by finding
// maddy-owned UDP sockets on ephemeral ports.
func countTurnRelays(knownPort string) int {
	cmd := exec.Command("ss", "-unap")
	output, err := cmd.Output()
	if err != nil {
		return 0
	}
	count := 0
	scanner := bufio.NewScanner(strings.NewReader(string(output)))
	for scanner.Scan() {
		line := scanner.Text()
		if !strings.Contains(line, "\"maddy\"") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 5 {
			continue
		}
		localPort := extractPortFromAddr(fields[3])
		if localPort == knownPort {
			continue
		}
		count++
	}
	return count
}

// extractIPFromAddr extracts IP from addr:port or [ipv6]:port.
func extractIPFromAddr(addr string) string {
	if strings.HasPrefix(addr, "[") {
		idx := strings.LastIndex(addr, "]:")
		if idx != -1 {
			return addr[1:idx]
		}
		return strings.Trim(addr, "[]")
	}
	if strings.Count(addr, ":") > 1 {
		return addr // IPv6 without brackets
	}
	idx := strings.LastIndex(addr, ":")
	if idx != -1 {
		return addr[:idx]
	}
	return addr
}

// extractPortFromAddr extracts port from addr:port.
func extractPortFromAddr(addr string) string {
	idx := strings.LastIndex(addr, ":")
	if idx == -1 {
		return addr
	}
	return addr[idx+1:]
}

// StorageResponse is the response body for /admin/storage.
type StorageResponse struct {
	Disk     *DiskInfo     `json:"disk"`
	StateDir *StateDirInfo `json:"state_dir"`
	Database *DatabaseInfo `json:"database,omitempty"`
}

type DiskInfo struct {
	TotalBytes     uint64  `json:"total_bytes"`
	UsedBytes      uint64  `json:"used_bytes"`
	AvailableBytes uint64  `json:"available_bytes"`
	PercentUsed    float64 `json:"percent_used"`
}

type StateDirInfo struct {
	Path      string `json:"path"`
	SizeBytes int64  `json:"size_bytes"`
}

type DatabaseInfo struct {
	Driver    string `json:"driver"`
	SizeBytes int64  `json:"size_bytes"`
}

// StorageDeps are the dependencies needed by the storage resource handler.
type StorageDeps struct {
	StateDir string
	DBDriver string
	DBDSN    string
}

// StorageHandler creates a handler for /admin/storage.
func StorageHandler(deps StorageDeps) func(method string, body json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if method != "GET" {
			return nil, 405, fmt.Errorf("method %s not allowed, use GET", method)
		}

		resp := StorageResponse{}

		// Disk usage via statfs on the state directory
		stateDir := deps.StateDir
		if stateDir == "" {
			stateDir = config.StateDirectory
		}

		var stat syscall.Statfs_t
		if err := syscall.Statfs(stateDir, &stat); err == nil {
			totalBytes := stat.Blocks * uint64(stat.Bsize)
			availBytes := stat.Bavail * uint64(stat.Bsize)
			usedBytes := totalBytes - availBytes
			pct := float64(0)
			if totalBytes > 0 {
				pct = float64(usedBytes) / float64(totalBytes) * 100
			}
			resp.Disk = &DiskInfo{
				TotalBytes:     totalBytes,
				UsedBytes:      usedBytes,
				AvailableBytes: availBytes,
				PercentUsed:    pct,
			}
		}

		// State directory size
		resp.StateDir = &StateDirInfo{Path: stateDir}
		var dirSize int64
		_ = filepath.Walk(stateDir, func(_ string, info os.FileInfo, err error) error {
			if err != nil {
				return nil
			}
			if !info.IsDir() {
				dirSize += info.Size()
			}
			return nil
		})
		resp.StateDir.SizeBytes = dirSize

		// Database size (for sqlite, the file size)
		if deps.DBDriver == "sqlite3" || deps.DBDriver == "sqlite" {
			if info, err := os.Stat(filepath.Join(stateDir, deps.DBDSN)); err == nil {
				resp.Database = &DatabaseInfo{
					Driver:    deps.DBDriver,
					SizeBytes: info.Size(),
				}
			}
		} else if deps.DBDriver != "" {
			resp.Database = &DatabaseInfo{Driver: deps.DBDriver}
		}

		return resp, 200, nil
	}
}

func formatDuration(d time.Duration) string {
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
	return fmt.Sprintf("%dm %ds", minutes, seconds)
}

// RestartHandler creates a handler for POST /admin/restart.
// It schedules a service restart via systemctl after a short delay
// so the HTTP response can be sent back to the client first.
func RestartHandler() func(method string, body json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if method != "POST" {
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}

		// Schedule restart after a short delay so the response gets sent first
		go func() {
			time.Sleep(500 * time.Millisecond)
			_ = exec.Command("systemctl", "restart", "maddy.service").Run()
		}()

		return map[string]string{
			"status":  "restarting",
			"message": "Service restart initiated. Please wait a few seconds.",
		}, 200, nil
	}
}
