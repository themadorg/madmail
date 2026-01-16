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

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/themadorg/madmail/internal/target/queue"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(
		&cli.Command{
			Name:  "queue",
			Usage: "Delivery queue management",
			Subcommands: []*cli.Command{
				{
					Name:      "purge",
					Usage:     "Delete messages from queue for a user",
					ArgsUsage: "USERNAME",
					Flags: []cli.Flag{
						&cli.StringFlag{
							Name:    "cfg-block",
							Usage:   "Module configuration block to use",
							EnvVars: []string{"MADDY_CFGBLOCK"},
							Value:   "remote_queue",
						},
						&cli.BoolFlag{
							Name:  "sender",
							Usage: "Purge messages from this sender",
							Value: true,
						},
						&cli.BoolFlag{
							Name:  "recipient",
							Usage: "Purge messages to this recipient",
							Value: true,
						},
					},
					Action: func(ctx *cli.Context) error {
						q, err := openQueueTarget(ctx)
						if err != nil {
							return err
						}
						return queuePurge(q, ctx)
					},
				},
			},
		})
}

func queuePurge(q *queue.Queue, ctx *cli.Context) error {
	username := ctx.Args().First()
	if username == "" {
		return cli.Exit("Error: USERNAME is required", 2)
	}

	total := 0
	if ctx.Bool("sender") {
		n, err := q.PurgeBySender(username)
		if err != nil {
			return err
		}
		total += n
	}

	if ctx.Bool("recipient") {
		n, err := q.PurgeByRecipient(username)
		if err != nil {
			return err
		}
		total += n
	}

	fmt.Printf("Purged %d messages for %s from queue.\n", total, username)
	return nil
}
