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
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"text/tabwriter"

	_ "github.com/mattn/go-sqlite3"
	maddy "github.com/themadorg/madmail"
	parser "github.com/themadorg/madmail/framework/cfgparser"
	"github.com/themadorg/madmail/framework/config"
	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "sharing",
		Usage: "DeltaChat contact sharing management",
		Description: `These commands allow you to manage the shareable DeltaChat contact links.
You can create, delete, list, and edit links from the terminal.

Examples:
	maddy sharing create myname https://i.delta.chat/#... "My Name"
	maddy sharing delete myname
	maddy sharing list
	maddy sharing reserve secret
`,
		Subcommands: []*cli.Command{
			{
				Name:  "list",
				Usage: "List all contact share links",
				Action: func(ctx *cli.Context) error {
					db, err := openSharingDB(ctx)
					if err != nil {
						return err
					}
					defer db.Close()
					return sharingList(db)
				},
			},
			{
				Name:      "create",
				Usage:     "Create a new share link",
				ArgsUsage: "SLUG URL [NAME]",
				Description: `URL must be a DeltaChat web invitation link (https://i.delta.chat/#...).
SLUG must be alphanumeric.`,
				Action: func(ctx *cli.Context) error {
					if ctx.NArg() < 2 {
						return cli.Exit("Error: SLUG and URL are required", 2)
					}
					db, err := openSharingDB(ctx)
					if err != nil {
						return err
					}
					defer db.Close()
					return sharingCreate(db, ctx)
				},
			},
			{
				Name:      "reserve",
				Usage:     "Reserve a slug (creates a link pointing to 'reserved')",
				ArgsUsage: "SLUG",
				Action: func(ctx *cli.Context) error {
					if ctx.NArg() < 1 {
						return cli.Exit("Error: SLUG is required", 2)
					}
					db, err := openSharingDB(ctx)
					if err != nil {
						return err
					}
					defer db.Close()
					return sharingCreateInternal(db, ctx.Args().First(), "reserved", "Reserved")
				},
			},
			{
				Name:      "remove",
				Aliases:   []string{"delete"},
				Usage:     "Remove a share link",
				ArgsUsage: "SLUG",
				Action: func(ctx *cli.Context) error {
					if ctx.NArg() < 1 {
						return cli.Exit("Error: SLUG is required", 2)
					}
					db, err := openSharingDB(ctx)
					if err != nil {
						return err
					}
					defer db.Close()
					return sharingRemove(db, ctx.Args().First())
				},
			},
			{
				Name:      "edit",
				Usage:     "Edit an existing share link",
				ArgsUsage: "SLUG NEW_URL [NEW_NAME]",
				Action: func(ctx *cli.Context) error {
					if ctx.NArg() < 2 {
						return cli.Exit("Error: SLUG and NEW_URL are required", 2)
					}
					db, err := openSharingDB(ctx)
					if err != nil {
						return err
					}
					defer db.Close()
					return sharingEdit(db, ctx)
				},
			},
		},
	})
}

func openSharingDB(ctx *cli.Context) (*sql.DB, error) {
	cfgPath := ctx.String("config")
	if cfgPath == "" {
		return nil, cli.Exit("Error: config is required", 2)
	}
	cfgFile, err := os.Open(cfgPath)
	if err != nil {
		return nil, cli.Exit(fmt.Sprintf("Error: failed to open config: %v", err), 2)
	}
	defer cfgFile.Close()
	cfgNodes, err := parser.Read(cfgFile, cfgFile.Name())
	if err != nil {
		return nil, cli.Exit(fmt.Sprintf("Error: failed to parse config: %v", err), 2)
	}

	_, _, err = maddy.ReadGlobals(cfgNodes)
	if err != nil {
		return nil, err
	}

	if config.StateDirectory == "" {
		config.StateDirectory = "/var/lib/maddy"
	}

	dbPath := filepath.Join(config.StateDirectory, "sharing.db")
	db, err := sql.Open("sqlite3", dbPath)
	if err != nil {
		return nil, fmt.Errorf("failed to open sharing DB: %v", err)
	}

	// Ensure table exists
	_, err = db.Exec(`CREATE TABLE IF NOT EXISTS contacts (
		slug TEXT PRIMARY KEY,
		url TEXT NOT NULL,
		name TEXT,
		created_at DATETIME DEFAULT CURRENT_TIMESTAMP
	)`)
	if err != nil {
		db.Close()
		return nil, fmt.Errorf("failed to create sharing table: %v", err)
	}

	return db, nil
}

func sharingList(db *sql.DB) error {
	rows, err := db.Query("SELECT slug, url, name, created_at FROM contacts ORDER BY created_at DESC")
	if err != nil {
		return err
	}
	defer rows.Close()

	w := tabwriter.NewWriter(os.Stdout, 0, 0, 3, ' ', 0)
	fmt.Fprintln(w, "SLUG\tNAME\tURL\tCREATED AT")
	for rows.Next() {
		var slug, url, name, createdAt string
		if err := rows.Scan(&slug, &url, &name, &createdAt); err != nil {
			return err
		}
		fmt.Fprintf(w, "%s\t%s\t%s\t%s\n", slug, name, url, createdAt)
	}
	return w.Flush()
}

func sharingCreate(db *sql.DB, ctx *cli.Context) error {
	slug := ctx.Args().Get(0)
	rawURL := ctx.Args().Get(1)
	name := ctx.Args().Get(2)
	return sharingCreateInternal(db, slug, rawURL, name)
}

func sharingCreateInternal(db *sql.DB, slug, rawURL, name string) error {
	// Validate slug
	for _, r := range slug {
		if !((r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9')) {
			return cli.Exit("Error: SLUG must be alphanumeric (a-z, 0-9)", 2)
		}
	}

	// Convert URL if it's a web link
	if strings.HasPrefix(rawURL, "https://i.delta.chat/#") {
		content := strings.TrimPrefix(rawURL, "https://i.delta.chat/#")
		if idx := strings.Index(content, "&"); idx != -1 {
			rawURL = "openpgp4fpr:" + content[:idx] + "#" + content[idx+1:]
		} else {
			rawURL = "openpgp4fpr:" + content
		}
	} else if rawURL != "reserved" && !strings.HasPrefix(rawURL, "openpgp4fpr:") {
		return cli.Exit("Error: URL must be DeltaChat web link (https://i.delta.chat/#...) or openpgp4fpr: link", 2)
	}

	_, err := db.Exec("INSERT INTO contacts (slug, url, name) VALUES (?, ?, ?)", slug, rawURL, name)
	if err != nil {
		return fmt.Errorf("failed to create link: %v", err)
	}
	fmt.Printf("Successfully created link: %s\n", slug)
	return nil
}

func sharingRemove(db *sql.DB, slug string) error {
	res, err := db.Exec("DELETE FROM contacts WHERE slug = ?", slug)
	if err != nil {
		return err
	}
	affected, _ := res.RowsAffected()
	if affected == 0 {
		return cli.Exit(fmt.Sprintf("Error: slug %s not found", slug), 2)
	}
	fmt.Printf("Successfully removed link: %s\n", slug)
	return nil
}

func sharingEdit(db *sql.DB, ctx *cli.Context) error {
	slug := ctx.Args().Get(0)
	rawURL := ctx.Args().Get(1)
	name := ctx.Args().Get(2)

	// Convert URL if it's a web link
	if strings.HasPrefix(rawURL, "https://i.delta.chat/#") {
		content := strings.TrimPrefix(rawURL, "https://i.delta.chat/#")
		if idx := strings.Index(content, "&"); idx != -1 {
			rawURL = "openpgp4fpr:" + content[:idx] + "#" + content[idx+1:]
		} else {
			rawURL = "openpgp4fpr:" + content
		}
	}

	var res sql.Result
	var err error
	if name != "" {
		res, err = db.Exec("UPDATE contacts SET url = ?, name = ? WHERE slug = ?", rawURL, name, slug)
	} else {
		res, err = db.Exec("UPDATE contacts SET url = ? WHERE slug = ?", rawURL, slug)
	}

	if err != nil {
		return err
	}
	affected, _ := res.RowsAffected()
	if affected == 0 {
		return cli.Exit(fmt.Sprintf("Error: slug %s not found", slug), 2)
	}
	fmt.Printf("Successfully updated link: %s\n", slug)
	return nil
}
