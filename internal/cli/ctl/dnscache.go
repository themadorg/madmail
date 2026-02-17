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
	"text/tabwriter"
	"time"

	_ "github.com/mattn/go-sqlite3"
	maddy "github.com/themadorg/madmail"
	parser "github.com/themadorg/madmail/framework/cfgparser"
	"github.com/themadorg/madmail/framework/config"
	maddycli "github.com/themadorg/madmail/internal/cli"
	mdb "github.com/themadorg/madmail/internal/db"
	"github.com/urfave/cli/v2"
	"gorm.io/gorm"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "dns-cache",
		Usage: "DNS override cache management",
		Description: `These commands allow you to manage the internal DNS override cache.
The DNS cache intercepts outbound mail delivery DNS resolution and allows
you to redirect delivery to specific hosts without modifying system DNS.

Examples:
	maddy dns-cache list
	maddy dns-cache set nine.testrun.org 10.0.0.5 "Route to staging"
	maddy dns-cache set 1.1.1.1 2.2.2.2 "Redirect IP"
	maddy dns-cache get nine.testrun.org
	maddy dns-cache remove nine.testrun.org
`,
		Subcommands: []*cli.Command{
			{
				Name:  "list",
				Usage: "List all DNS override entries",
				Action: func(ctx *cli.Context) error {
					db, err := openDNSCacheGORM(ctx)
					if err != nil {
						return err
					}
					return dnsCacheList(db)
				},
			},
			{
				Name:      "set",
				Usage:     "Create or update a DNS override entry",
				ArgsUsage: "LOOKUP_KEY TARGET_HOST [COMMENT]",
				Description: `LOOKUP_KEY is the domain name or IP address to override.
TARGET_HOST is the destination host/IP to redirect to.
COMMENT is an optional human-readable note.

Examples:
  maddy dns-cache set example.com 10.0.0.1 "Route to internal server"
  maddy dns-cache set 1.1.1.1 2.2.2.2 "Redirect IP literal"
  maddy dns-cache set nine.testrun.org new-mx.example.com "Migration"`,
				Action: func(ctx *cli.Context) error {
					if ctx.NArg() < 2 {
						return cli.Exit("Error: LOOKUP_KEY and TARGET_HOST are required", 2)
					}
					db, err := openDNSCacheGORM(ctx)
					if err != nil {
						return err
					}
					return dnsCacheSet(db, ctx)
				},
			},
			{
				Name:      "get",
				Usage:     "Show a specific DNS override entry",
				ArgsUsage: "LOOKUP_KEY",
				Action: func(ctx *cli.Context) error {
					if ctx.NArg() < 1 {
						return cli.Exit("Error: LOOKUP_KEY is required", 2)
					}
					db, err := openDNSCacheGORM(ctx)
					if err != nil {
						return err
					}
					return dnsCacheGet(db, ctx.Args().First())
				},
			},
			{
				Name:      "remove",
				Aliases:   []string{"delete"},
				Usage:     "Remove a DNS override entry",
				ArgsUsage: "LOOKUP_KEY",
				Action: func(ctx *cli.Context) error {
					if ctx.NArg() < 1 {
						return cli.Exit("Error: LOOKUP_KEY is required", 2)
					}
					db, err := openDNSCacheGORM(ctx)
					if err != nil {
						return err
					}
					return dnsCacheRemove(db, ctx.Args().First())
				},
			},
		},
	})
}

func openDNSCacheGORM(ctx *cli.Context) (*gorm.DB, error) {
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

	// Find the storage.imapsql block to get the standard driver and dsn.
	// This ensures the CLI connects to the same database as the running server.
	var driver string
	var dsn []string

	for _, node := range cfgNodes {
		if node.Name == "storage.imapsql" {
			for _, child := range node.Children {
				if child.Name == "driver" && len(child.Args) > 0 {
					driver = child.Args[0]
				}
				if child.Name == "dsn" {
					dsn = child.Args
				}
			}
			break
		}
	}

	if driver == "" || dsn == nil {
		return nil, cli.Exit("Error: could not find storage.imapsql block with driver and dsn in config", 2)
	}

	db, err := mdb.New(driver, dsn, ctx.Bool("debug"))
	if err != nil {
		return nil, fmt.Errorf("failed to open storage GORM DB: %v", err)
	}

	if err := db.AutoMigrate(&mdb.DNSOverride{}); err != nil {
		return nil, fmt.Errorf("failed to migrate DNS override table: %v", err)
	}

	return db, nil
}

func dnsCacheList(db *gorm.DB) error {
	var overrides []mdb.DNSOverride
	if err := db.Order("created_at DESC").Find(&overrides).Error; err != nil {
		return err
	}

	if len(overrides) == 0 {
		fmt.Fprintln(os.Stderr, "No DNS override entries.")
		return nil
	}

	w := tabwriter.NewWriter(os.Stdout, 0, 0, 3, ' ', 0)
	fmt.Fprintln(w, "LOOKUP KEY\tTARGET HOST\tCOMMENT\tCREATED AT\tUPDATED AT")
	for _, o := range overrides {
		fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\n",
			o.LookupKey,
			o.TargetHost,
			o.Comment,
			o.CreatedAt.Format(time.RFC3339),
			o.UpdatedAt.Format(time.RFC3339),
		)
	}
	return w.Flush()
}

func dnsCacheSet(db *gorm.DB, ctx *cli.Context) error {
	lookupKey := ctx.Args().Get(0)
	targetHost := ctx.Args().Get(1)
	comment := ctx.Args().Get(2)

	override := mdb.DNSOverride{
		LookupKey:  lookupKey,
		TargetHost: targetHost,
		Comment:    comment,
	}

	// Use Save which does upsert (create or update)
	if err := db.Save(&override).Error; err != nil {
		return fmt.Errorf("failed to set DNS override: %v", err)
	}
	fmt.Printf("Successfully set DNS override: %s → %s\n", lookupKey, targetHost)
	return nil
}

func dnsCacheGet(db *gorm.DB, lookupKey string) error {
	var override mdb.DNSOverride
	err := db.Where("lookup_key = ?", lookupKey).First(&override).Error
	if err != nil {
		if err == gorm.ErrRecordNotFound {
			return cli.Exit(fmt.Sprintf("Error: no DNS override found for %q", lookupKey), 2)
		}
		return err
	}

	w := tabwriter.NewWriter(os.Stdout, 0, 0, 3, ' ', 0)
	fmt.Fprintf(w, "Lookup Key:\t%s\n", override.LookupKey)
	fmt.Fprintf(w, "Target Host:\t%s\n", override.TargetHost)
	fmt.Fprintf(w, "Comment:\t%s\n", override.Comment)
	fmt.Fprintf(w, "Created At:\t%s\n", override.CreatedAt.Format(time.RFC3339))
	fmt.Fprintf(w, "Updated At:\t%s\n", override.UpdatedAt.Format(time.RFC3339))
	return w.Flush()
}

func dnsCacheRemove(db *gorm.DB, lookupKey string) error {
	result := db.Where("lookup_key = ?", lookupKey).Delete(&mdb.DNSOverride{})
	if result.Error != nil {
		return result.Error
	}
	if result.RowsAffected == 0 {
		return cli.Exit(fmt.Sprintf("Error: no DNS override found for %q", lookupKey), 2)
	}
	fmt.Printf("Successfully removed DNS override: %s\n", lookupKey)
	return nil
}
