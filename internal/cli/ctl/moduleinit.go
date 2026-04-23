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
	"errors"
	"fmt"
	"io"
	"os"
	"sync"

	maddy "github.com/themadorg/madmail"
	parser "github.com/themadorg/madmail/framework/cfgparser"
	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/target/queue"
	"github.com/themadorg/madmail/internal/updatepipe"
	"github.com/urfave/cli/v2"
)

func closeIfNeeded(i interface{}) {
	if c, ok := i.(io.Closer); ok {
		c.Close()
	}
}

// Module registration is a process-global side effect: maddy.RegisterModules
// refuses to register the same instance twice. Commands that open more than
// one config block in the same process (e.g. "maddy accounts status" opens
// both local_authdb and local_mailboxes) would otherwise fail on the second
// call with "config block named X already exists". We parse and register
// once, then look up blocks from the cached result.
var (
	modulesOnce   sync.Once
	cachedGlobals map[string]interface{}
	cachedMods    []maddy.ModInfo
	cachedModErr  error
)

func loadModules(ctx *cli.Context) (map[string]interface{}, []maddy.ModInfo, error) {
	modulesOnce.Do(func() {
		cfgPath := ctx.String("config")
		if cfgPath == "" {
			cachedModErr = cli.Exit("Error: config is required", 2)
			return
		}
		cfgFile, err := os.Open(cfgPath)
		if err != nil {
			cachedModErr = cli.Exit(fmt.Sprintf("Error: failed to open config: %v", err), 2)
			return
		}
		defer cfgFile.Close()
		cfgNodes, err := parser.Read(cfgFile, cfgFile.Name())
		if err != nil {
			cachedModErr = cli.Exit(fmt.Sprintf("Error: failed to parse config: %v", err), 2)
			return
		}

		globals, cfgNodes, err := maddy.ReadGlobals(cfgNodes)
		if err != nil {
			cachedModErr = err
			return
		}

		if err := maddy.InitDirs(); err != nil {
			cachedModErr = err
			return
		}

		module.NoRun = true
		_, mods, err := maddy.RegisterModules(globals, cfgNodes)
		if err != nil {
			cachedModErr = err
			return
		}

		cachedGlobals = globals
		cachedMods = mods
	})
	return cachedGlobals, cachedMods, cachedModErr
}

// getCfgBlockModuleFor loads the module instance for a named top-level configuration block.
func getCfgBlockModuleFor(ctx *cli.Context, cfgBlock string) (map[string]interface{}, *maddy.ModInfo, error) {
	globals, mods, err := loadModules(ctx)
	if err != nil {
		return nil, nil, err
	}

	if cfgBlock == "" {
		return nil, nil, cli.Exit("Error: cfg-block is required", 2)
	}
	for _, m := range mods {
		if m.Instance.InstanceName() == cfgBlock {
			mod := m
			return globals, &mod, nil
		}
	}
	return nil, nil, cli.Exit(fmt.Sprintf("Error: unknown configuration block: %s", cfgBlock), 2)
}

func openStorage(ctx *cli.Context) (module.Storage, error) {
	return openStorageForBlock(ctx, ctx.String("cfg-block"))
}

func openStorageForBlock(ctx *cli.Context, cfgBlock string) (module.Storage, error) {
	globals, mod, err := getCfgBlockModuleFor(ctx, cfgBlock)
	if err != nil {
		return nil, err
	}

	storage, ok := mod.Instance.(module.Storage)
	if !ok {
		return nil, cli.Exit(fmt.Sprintf("Error: configuration block %s is not an IMAP storage", cfgBlock), 2)
	}

	if !module.Initialized[mod.Instance.InstanceName()] {
		if err := mod.Instance.Init(config.NewMap(globals, mod.Cfg)); err != nil {
			return nil, fmt.Errorf("Error: module initialization failed: %w", err)
		}
		module.Initialized[mod.Instance.InstanceName()] = true
	}

	if updStore, ok := mod.Instance.(updatepipe.Backend); ok {
		if err := updStore.EnableUpdatePipe(updatepipe.ModePush); err != nil && !errors.Is(err, os.ErrNotExist) {
			fmt.Fprintf(os.Stderr, "Failed to initialize update pipe, do not remove messages from mailboxes open by clients: %v\n", err)
		}
	} else {
		fmt.Fprintf(os.Stderr, "No update pipe support, do not remove messages from mailboxes open by clients\n")
	}

	return storage, nil
}

func openUserDB(ctx *cli.Context) (module.PlainUserDB, error) {
	return openUserDBForBlock(ctx, ctx.String("cfg-block"))
}

func openUserDBForBlock(ctx *cli.Context, cfgBlock string) (module.PlainUserDB, error) {
	globals, mod, err := getCfgBlockModuleFor(ctx, cfgBlock)
	if err != nil {
		return nil, err
	}

	userDB, ok := mod.Instance.(module.PlainUserDB)
	if !ok {
		return nil, cli.Exit(fmt.Sprintf("Error: configuration block %s is not a local credentials store", cfgBlock), 2)
	}

	if !module.Initialized[mod.Instance.InstanceName()] {
		if err := mod.Instance.Init(config.NewMap(globals, mod.Cfg)); err != nil {
			return nil, fmt.Errorf("Error: module initialization failed: %w", err)
		}
		module.Initialized[mod.Instance.InstanceName()] = true
	}

	return userDB, nil
}
func openQueueTarget(ctx *cli.Context) (*queue.Queue, error) {
	globals, mod, err := getCfgBlockModuleFor(ctx, ctx.String("cfg-block"))
	if err != nil {
		return nil, err
	}

	q, ok := mod.Instance.(*queue.Queue)
	if !ok {
		return nil, cli.Exit(fmt.Sprintf("Error: configuration block %s is not a delivery queue", ctx.String("cfg-block")), 2)
	}

	if !module.Initialized[mod.Instance.InstanceName()] {
		if err := mod.Instance.Init(config.NewMap(globals, mod.Cfg)); err != nil {
			return nil, fmt.Errorf("Error: module initialization failed: %w", err)
		}
		module.Initialized[mod.Instance.InstanceName()] = true
	}

	return q, nil
}
