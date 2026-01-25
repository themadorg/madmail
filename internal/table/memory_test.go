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
	"testing"
)

func TestMemoryTable(t *testing.T) {
	mem, err := NewMemory("table.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create memory table: %v", err)
	}

	mtbl := mem.(*Memory)

	// Test SetKey and Lookup
	if err := mtbl.SetKey("test_key", "test_value"); err != nil {
		t.Fatalf("SetKey failed: %v", err)
	}

	val, ok, err := mtbl.Lookup(context.Background(), "test_key")
	if err != nil {
		t.Fatalf("Lookup failed: %v", err)
	}
	if !ok {
		t.Fatal("Expected key to be found")
	}
	if val != "test_value" {
		t.Fatalf("Expected 'test_value', got '%s'", val)
	}

	// Test non-existent key
	_, ok, err = mtbl.Lookup(context.Background(), "non_existent")
	if err != nil {
		t.Fatalf("Lookup failed: %v", err)
	}
	if ok {
		t.Fatal("Expected key not to be found")
	}

	// Test Keys
	if err := mtbl.SetKey("key1", "value1"); err != nil {
		t.Fatalf("SetKey failed: %v", err)
	}
	if err := mtbl.SetKey("key2", "value2"); err != nil {
		t.Fatalf("SetKey failed: %v", err)
	}

	keys, err := mtbl.Keys()
	if err != nil {
		t.Fatalf("Keys failed: %v", err)
	}
	if len(keys) != 3 {
		t.Fatalf("Expected 3 keys, got %d", len(keys))
	}

	// Test RemoveKey
	if err := mtbl.RemoveKey("test_key"); err != nil {
		t.Fatalf("RemoveKey failed: %v", err)
	}

	_, ok, err = mtbl.Lookup(context.Background(), "test_key")
	if err != nil {
		t.Fatalf("Lookup failed: %v", err)
	}
	if ok {
		t.Fatal("Expected key to be removed")
	}

	keys, err = mtbl.Keys()
	if err != nil {
		t.Fatalf("Keys failed: %v", err)
	}
	if len(keys) != 2 {
		t.Fatalf("Expected 2 keys after removal, got %d", len(keys))
	}
}

func TestMemoryConcurrency(t *testing.T) {
	mem, err := NewMemory("table.memory", "test", nil, nil)
	if err != nil {
		t.Fatalf("Failed to create memory table: %v", err)
	}

	mtbl := mem.(*Memory)

	// Test concurrent writes and reads
	done := make(chan bool)
	for i := 0; i < 10; i++ {
		go func(idx int) {
			key := "concurrent_key"
			val := "value"
			if err := mtbl.SetKey(key, val); err != nil {
				t.Errorf("SetKey failed: %v", err)
			}
			if _, _, err := mtbl.Lookup(context.Background(), key); err != nil {
				t.Errorf("Lookup failed: %v", err)
			}
			done <- true
		}(i)
	}

	for i := 0; i < 10; i++ {
		<-done
	}
}
