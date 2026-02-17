package resources

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"syscall"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/internal/servertracker"
)

// StatusResponse is the response body for /admin/status.
type StatusResponse struct {
	IMAP         *ServiceStatus `json:"imap,omitempty"`
	TURN         *TurnStatus    `json:"turn,omitempty"`
	Shadowsocks  *ServiceStatus `json:"shadowsocks,omitempty"`
	Users        *UsersStatus   `json:"users"`
	Uptime       *UptimeStatus  `json:"uptime"`
	EmailServers *EmailServers  `json:"email_servers,omitempty"`
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

		// Server tracker status
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

		return resp, 200, nil
	}
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
