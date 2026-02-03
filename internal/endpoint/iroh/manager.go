package iroh

import (
	"fmt"
	"os"
	"path/filepath"
)

// ExtractBinary extracts the embedded iroh-relay binary to the specified path.
func ExtractBinary(destPath string) error {
	if err := os.MkdirAll(filepath.Dir(destPath), 0755); err != nil {
		return fmt.Errorf("failed to create directory for iroh-relay: %v", err)
	}

	// Remove existing binary to avoid "text file busy" errors if it's already running
	if err := os.Remove(destPath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to remove existing iroh-relay binary: %v", err)
	}

	if err := os.WriteFile(destPath, IrohRelayBinary, 0755); err != nil {
		return fmt.Errorf("failed to write iroh-relay binary: %v", err)
	}

	return nil
}
