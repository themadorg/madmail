//go:build !linux
// +build !linux

package ctl

// reloadRunningDaemons is a no-op on non-Linux platforms. Operators should
// send SIGUSR2 to the madmail daemon manually (e.g. `kill -USR2 <pid>` or
// `systemctl kill -s USR2 madmail.service`) after CLI mutations.
func reloadRunningDaemons() ([]int, error) {
	return nil, nil
}
