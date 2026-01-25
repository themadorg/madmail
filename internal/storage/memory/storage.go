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

// Package memory implements an in-memory storage module for maddy.
package memory

import (
	"errors"
	"fmt"
	"sync"
	"time"

	imapbackend "github.com/emersion/go-imap/backend"
	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
)

// Storage implements in-memory IMAP storage.
type Storage struct {
	modName  string
	instName string
	log      log.Logger

	mu       sync.RWMutex
	users    map[string]*User
	quotas   map[string]*QuotaInfo
	accounts map[string]*AccountInfo

	defaultQuota int64
	autoCreate   bool
}

type QuotaInfo struct {
	Used      int64
	Max       int64
	IsDefault bool
}

type AccountInfo struct {
	Created      int64
	FirstLoginAt int64
}

// New creates a new in-memory storage backend.
func New(modName, instName string, _, _ []string) (module.Module, error) {
	return &Storage{
		modName:      modName,
		instName:     instName,
		users:        make(map[string]*User),
		quotas:       make(map[string]*QuotaInfo),
		accounts:     make(map[string]*AccountInfo),
		defaultQuota: 1024 * 1024 * 1024, // 1GB default
	}, nil
}

func (s *Storage) Init(cfg *config.Map) error {
	s.log = log.Logger{Name: s.modName}

	cfg.Int64("default_quota", false, false, 1024*1024*1024, &s.defaultQuota)
	cfg.Bool("auto_create", false, false, &s.autoCreate)

	_, err := cfg.Process()
	return err
}

func (s *Storage) Name() string {
	return s.modName
}

func (s *Storage) InstanceName() string {
	return s.instName
}

// GetOrCreateIMAPAcct implements module.Storage
func (s *Storage) GetOrCreateIMAPAcct(username string) (imapbackend.User, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	user, exists := s.users[username]
	if !exists {
		if !s.autoCreate {
			return nil, errors.New("account does not exist")
		}
		user = newUser(username, s)
		s.users[username] = user
		s.accounts[username] = &AccountInfo{
			Created:      time.Now().Unix(),
			FirstLoginAt: 1, // Marker for "not logged in yet"
		}
		s.quotas[username] = &QuotaInfo{
			Used:      0,
			Max:       s.defaultQuota,
			IsDefault: true,
		}
	}
	return user, nil
}

// GetIMAPAcct implements module.Storage
func (s *Storage) GetIMAPAcct(username string) (imapbackend.User, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	user, exists := s.users[username]
	if !exists {
		return nil, errors.New("account does not exist")
	}
	return user, nil
}

// IMAPExtensions implements module.Storage
func (s *Storage) IMAPExtensions() []string {
	return []string{"IDLE", "UNSELECT", "UIDPLUS", "CHILDREN", "NAMESPACE"}
}

// ListIMAPAccts implements module.ManageableStorage
func (s *Storage) ListIMAPAccts() ([]string, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	accounts := make([]string, 0, len(s.users))
	for username := range s.users {
		accounts = append(accounts, username)
	}
	return accounts, nil
}

// CreateIMAPAcct implements module.ManageableStorage
func (s *Storage) CreateIMAPAcct(username string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if _, exists := s.users[username]; exists {
		return fmt.Errorf("account %s already exists", username)
	}

	user := newUser(username, s)
	s.users[username] = user
	s.accounts[username] = &AccountInfo{
		Created:      time.Now().Unix(),
		FirstLoginAt: 1,
	}
	s.quotas[username] = &QuotaInfo{
		Used:      0,
		Max:       s.defaultQuota,
		IsDefault: true,
	}
	return nil
}

// DeleteIMAPAcct implements module.ManageableStorage
func (s *Storage) DeleteIMAPAcct(username string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if _, exists := s.users[username]; !exists {
		return fmt.Errorf("account %s does not exist", username)
	}

	delete(s.users, username)
	delete(s.accounts, username)
	delete(s.quotas, username)
	return nil
}

// PurgeIMAPMsgs implements module.ManageableStorage
func (s *Storage) PurgeIMAPMsgs(username string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	user, exists := s.users[username]
	if !exists {
		return fmt.Errorf("account %s does not exist", username)
	}

	// Clear all messages in all mailboxes
	user.mu.Lock()
	defer user.mu.Unlock()

	for _, mbox := range user.mailboxes {
		mbox.mu.Lock()
		mbox.messages = nil
		mbox.nextUID = 1
		mbox.mu.Unlock()
	}

	// Reset quota usage
	if quota, ok := s.quotas[username]; ok {
		quota.Used = 0
	}

	return nil
}

// GetQuota implements module.ManageableStorage
func (s *Storage) GetQuota(username string) (used, max int64, isDefault bool, err error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	quota, exists := s.quotas[username]
	if !exists {
		return 0, s.defaultQuota, true, nil
	}

	return quota.Used, quota.Max, quota.IsDefault, nil
}

// SetQuota implements module.ManageableStorage
func (s *Storage) SetQuota(username string, max int64) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	quota, exists := s.quotas[username]
	if !exists {
		s.quotas[username] = &QuotaInfo{
			Used:      0,
			Max:       max,
			IsDefault: false,
		}
		return nil
	}

	quota.Max = max
	quota.IsDefault = false
	return nil
}

// ResetQuota implements module.ManageableStorage
func (s *Storage) ResetQuota(username string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	quota, exists := s.quotas[username]
	if !exists {
		return nil
	}

	quota.Max = s.defaultQuota
	quota.IsDefault = true
	return nil
}

// GetAccountDate implements module.ManageableStorage
func (s *Storage) GetAccountDate(username string) (created int64, err error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	account, exists := s.accounts[username]
	if !exists {
		return 0, fmt.Errorf("account %s does not exist", username)
	}

	return account.Created, nil
}

// UpdateFirstLogin implements module.ManageableStorage
func (s *Storage) UpdateFirstLogin(username string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	account, exists := s.accounts[username]
	if !exists {
		return nil
	}

	if account.FirstLoginAt == 1 {
		account.FirstLoginAt = time.Now().Unix()
	}

	return nil
}

// PruneUnusedAccounts implements module.ManageableStorage
func (s *Storage) PruneUnusedAccounts(retention time.Duration) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	now := time.Now().Unix()
	toDelete := []string{}

	for username, account := range s.accounts {
		// Skip if account has logged in
		if account.FirstLoginAt != 1 {
			continue
		}

		// Check if account is older than retention period
		age := now - account.Created
		if age > int64(retention.Seconds()) {
			toDelete = append(toDelete, username)
		}
	}

	// Delete old unused accounts
	for _, username := range toDelete {
		delete(s.users, username)
		delete(s.accounts, username)
		delete(s.quotas, username)
	}

	return nil
}

// GetDefaultQuota implements module.ManageableStorage
func (s *Storage) GetDefaultQuota() int64 {
	return s.defaultQuota
}

// SetDefaultQuota implements module.ManageableStorage
func (s *Storage) SetDefaultQuota(max int64) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	s.defaultQuota = max
	return nil
}

// GetStat implements module.ManageableStorage
func (s *Storage) GetStat() (totalStorage int64, accountsCount int, err error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var total int64
	for _, quota := range s.quotas {
		total += quota.Used
	}

	return total, len(s.users), nil
}

func init() {
	module.Register("storage.memory", New)
}
