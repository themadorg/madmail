/*
Maddy Mail Server - Composable all-in-one email server.
Copyright © 2019-2020 Max Mazurov <fox.cpp@disroot.org>, Maddy Mail Server contributors

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

package ctl

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/endpoint/chatmail"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:        "html-export",
		Usage:       "Export default HTML files to a directory",
		ArgsUsage:   "DEST_DIR",
		Description: "Exports all embedded HTML, CSS, and SVG files from the chatmail endpoint to the specified directory.",
		Action: func(ctx *cli.Context) error {
			if ctx.NArg() < 1 {
				return cli.ShowCommandHelp(ctx, "html-export")
			}
			return htmlExport(ctx.Args().First())
		},
	})
	maddycli.AddSubcommand(&cli.Command{
		Name:        "html-serve",
		Usage:       "Configure maddy to serve HTML from a directory",
		ArgsUsage:   "WWW_DIR",
		Description: "Updates maddy.conf to serve chatmail HTML files from the specified external directory instead of embedded ones.",
		Action: func(ctx *cli.Context) error {
			if ctx.NArg() < 1 {
				return cli.ShowCommandHelp(ctx, "html-serve")
			}
			return htmlServe(ctx)
		},
	})
}

func htmlExport(destDir string) error {
	if err := os.MkdirAll(destDir, 0755); err != nil {
		return fmt.Errorf("failed to create directory: %v", err)
	}

	entries, err := chatmail.WWWFiles.ReadDir("www")
	if err != nil {
		return fmt.Errorf("failed to read embedded files: %v", err)
	}

	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		data, err := chatmail.WWWFiles.ReadFile("www/" + entry.Name())
		if err != nil {
			return fmt.Errorf("failed to read %s: %v", entry.Name(), err)
		}
		if err := os.WriteFile(filepath.Join(destDir, entry.Name()), data, 0644); err != nil {
			return fmt.Errorf("failed to write %s: %v", entry.Name(), err)
		}
		fmt.Printf("Exported: %s\n", entry.Name())
	}
	fmt.Printf("Successfully exported all files to %s\n", destDir)
	return nil
}

func htmlServe(ctx *cli.Context) error {
	wwwDir := ctx.Args().First()
	isEmbedded := wwwDir == "embedded" || wwwDir == "embed" || wwwDir == "internal"

	var absWWWDir string
	var err error
	if !isEmbedded {
		absWWWDir, err = filepath.Abs(wwwDir)
		if err != nil {
			return fmt.Errorf("failed to get absolute path: %v", err)
		}
	}

	cfgPath := ctx.String("config")
	if cfgPath == "" {
		return cli.Exit("Error: config is required", 2)
	}

	data, err := os.ReadFile(cfgPath)
	if err != nil {
		return fmt.Errorf("failed to read config: %v", err)
	}

	lines := strings.Split(string(data), "\n")
	var newLines []string
	updated := false
	inChatmail := false

	for i := 0; i < len(lines); i++ {
		line := lines[i]
		trimmed := strings.TrimSpace(line)

		// Detection of chatmail block start
		if strings.HasPrefix(trimmed, "chatmail ") && strings.HasSuffix(trimmed, "{") {
			inChatmail = true
			newLines = append(newLines, line)
			continue
		}

		if inChatmail && trimmed == "}" {
			if isEmbedded {
				// We don't add anything for embedded, and we should have filtered out www_dir
				updated = true
			} else {
				// Check if we already have www_dir in this block
				hasWWWDir := false
				for j := len(newLines) - 1; j >= 0; j-- {
					if strings.HasPrefix(strings.TrimSpace(newLines[j]), "chatmail ") {
						break
					}
					if strings.HasPrefix(strings.TrimSpace(newLines[j]), "www_dir ") {
						newLines[j] = "    www_dir " + absWWWDir
						hasWWWDir = true
						updated = true
						break
					}
				}
				if !hasWWWDir {
					newLines = append(newLines, "    www_dir "+absWWWDir)
					updated = true
				}
			}
			inChatmail = false
		}

		// Filter out existing www_dir if we are in chatmail block
		if inChatmail && strings.HasPrefix(trimmed, "www_dir ") {
			if isEmbedded {
				// Just omit it to "remove" it
				continue
			}
			// When updating, we replace it above at the end of the block or find it.
			// Actually, the logic above for updating is slightly flawed if we append it here too.
			// Let's just skip it here and the logic at '}' will handle re-insertion if needed.
			continue
		}

		newLines = append(newLines, line)
	}

	if !updated {
		return cli.Exit("Error: No chatmail block found in config or failed to update.", 2)
	}

	if err := os.WriteFile(cfgPath, []byte(strings.Join(newLines, "\n")), 0644); err != nil {
		return fmt.Errorf("failed to write config: %v", err)
	}

	if isEmbedded {
		fmt.Printf("Successfully updated %s to use embedded HTML files.\n", cfgPath)
	} else {
		fmt.Printf("Successfully updated %s to serve HTML from %s\n", cfgPath, absWWWDir)
		fmt.Println("\n⚠️  Note: Ensure the 'maddy' user has read access to this directory.")
		fmt.Println("If the directory is inside a home folder, you may need to move it to /var/lib/maddy/www.")
		fmt.Println("Example: sudo mv " + wwwDir + " /var/lib/maddy/www && sudo chown -R maddy:maddy /var/lib/maddy/www")
	}
	fmt.Println("\nPlease restart maddy to apply changes: sudo systemctl restart maddy")
	return nil
}
