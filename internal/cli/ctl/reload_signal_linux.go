//go:build linux
// +build linux

package ctl

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
)

// reloadRunningDaemons sends SIGUSR2 to every running madmail process (except
// the CLI itself). The daemon's signal handler (signal.go) fires EventReload,
// which pass_table and imapsql use to rehydrate their credentials and quota
// caches from SQL. Without this call, `maddy accounts ban / delete` edits
// disk but the running server keeps serving stale cached state.
//
// Returns the pids that were signalled so the caller can report them.
func reloadRunningDaemons() ([]int, error) {
	self := os.Getpid()
	entries, err := os.ReadDir("/proc")
	if err != nil {
		return nil, fmt.Errorf("scan /proc: %w", err)
	}

	var signalled []int
	for _, ent := range entries {
		if !ent.IsDir() {
			continue
		}
		pid, err := strconv.Atoi(ent.Name())
		if err != nil || pid == self {
			continue
		}
		comm, err := os.ReadFile(filepath.Join("/proc", ent.Name(), "comm"))
		if err != nil {
			// Process gone or unreadable — not fatal, just skip.
			continue
		}
		if strings.TrimSpace(string(comm)) != "madmail" {
			continue
		}
		// Confirm it's actually a daemon invocation ("madmail run ..."),
		// not another `madmail accounts ...` CLI running in parallel.
		cmdline, err := os.ReadFile(filepath.Join("/proc", ent.Name(), "cmdline"))
		if err != nil {
			continue
		}
		args := strings.Split(string(cmdline), "\x00")
		if !isDaemonInvocation(args) {
			continue
		}
		if err := syscall.Kill(pid, syscall.SIGUSR2); err != nil {
			// EPERM is the common case when the CLI user can't signal a
			// daemon owned by root (or vice versa); report but keep going.
			fmt.Fprintf(os.Stderr, "warning: could not signal pid %d: %v\n", pid, err)
			continue
		}
		signalled = append(signalled, pid)
	}
	return signalled, nil
}

// isDaemonInvocation returns true for `madmail run ...`. The zeroth arg may
// be a full path, so we only check that some later arg is "run".
func isDaemonInvocation(args []string) bool {
	for i, a := range args {
		if i == 0 {
			continue
		}
		if a == "run" {
			return true
		}
	}
	return false
}
