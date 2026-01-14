//go:build tools
// +build tools

package main

import (
	// tooling for development
	_ "github.com/golangci/golangci-lint/cmd/golangci-lint"
)
