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

package table

import (
	"context"
	"sync"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/module"
)

// Memory is an in-memory key-value table implementation.
// It implements the module.MutableTable interface.
type Memory struct {
	modName  string
	instName string

	mu sync.RWMutex
	m  map[string]string
}

func NewMemory(modName, instName string, _, _ []string) (module.Module, error) {
	return &Memory{
		modName:  modName,
		instName: instName,
		m:        make(map[string]string),
	}, nil
}

func (mem *Memory) Init(cfg *config.Map) error {
	cfg.Callback("entry", func(_ *config.Map, node config.Node) error {
		if len(node.Args) < 2 {
			return config.NodeErr(node, "expected at least one value")
		}
		mem.mu.Lock()
		defer mem.mu.Unlock()
		mem.m[node.Args[0]] = node.Args[1]
		return nil
	})
	_, err := cfg.Process()
	return err
}

func (mem *Memory) Name() string {
	return mem.modName
}

func (mem *Memory) InstanceName() string {
	return mem.instName
}

func (mem *Memory) Lookup(ctx context.Context, key string) (string, bool, error) {
	mem.mu.RLock()
	defer mem.mu.RUnlock()
	val, ok := mem.m[key]
	return val, ok, nil
}

func (mem *Memory) Keys() ([]string, error) {
	mem.mu.RLock()
	defer mem.mu.RUnlock()
	keys := make([]string, 0, len(mem.m))
	for k := range mem.m {
		keys = append(keys, k)
	}
	return keys, nil
}

func (mem *Memory) RemoveKey(k string) error {
	mem.mu.Lock()
	defer mem.mu.Unlock()
	delete(mem.m, k)
	return nil
}

func (mem *Memory) SetKey(k, v string) error {
	mem.mu.Lock()
	defer mem.mu.Unlock()
	mem.m[k] = v
	return nil
}

func init() {
	module.Register("table.memory", NewMemory)
}
