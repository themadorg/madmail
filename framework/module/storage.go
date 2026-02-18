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

package module

import (
	"time"

	imapbackend "github.com/emersion/go-imap/backend"
	"gorm.io/gorm"
)

// BlockedUserEntry represents a blocked user returned by ListBlockedUsers.
type BlockedUserEntry struct {
	Username  string
	Reason    string
	BlockedAt time.Time
}

// Storage interface is a slightly modified go-imap's Backend interface
// (authentication is removed).
//
// Modules implementing this interface should be registered with prefix
// "storage." in name.
type Storage interface {
	// GetOrCreateIMAPAcct returns User associated with storage account specified by
	// the name.
	//
	// If it doesn't exists - it should be created.
	GetOrCreateIMAPAcct(username string) (imapbackend.User, error)
	GetIMAPAcct(username string) (imapbackend.User, error)

	// Extensions returns list of IMAP extensions supported by backend.
	IMAPExtensions() []string
}

// ManageableStorage is an extended Storage interface that allows to
// list existing accounts, create and delete them.
type ManageableStorage interface {
	Storage

	ListIMAPAccts() ([]string, error)
	CreateIMAPAcct(username string) error
	DeleteIMAPAcct(username string) error
	PurgeIMAPMsgs(username string) error

	GetQuota(username string) (used, max int64, isDefault bool, err error)
	SetQuota(username string, max int64) error
	ResetQuota(username string) error
	GetAccountDate(username string) (created int64, err error)
	UpdateFirstLogin(username string) error
	PruneUnusedAccounts(retention time.Duration) error
	GetDefaultQuota() int64
	SetDefaultQuota(max int64) error
	GetStat() (totalStorage int64, accountsCount int, err error)
	GetAllUsedStorage() (map[string]int64, error)
	PurgeAllIMAPMsgs() error
	PurgeReadIMAPMsgs() error
	PruneUnreadIMAPMsgs(retention time.Duration) error

	// Blocklist management — prevents blocked usernames from re-registering.
	BlockUser(username, reason string) error
	UnblockUser(username string) error
	IsBlocked(username string) (bool, error)
	ListBlockedUsers() ([]BlockedUserEntry, error)

	// DeleteAccount performs a full account removal:
	// 1. Delete IMAP storage and all messages
	// 2. Delete quota record
	// 3. Block the username from re-registration
	DeleteAccount(username, reason string) error
}

// GORMProvider is an optional interface that storage modules can implement
// to expose their GORM database connection for shared table access.
// Other modules (e.g. target.remote for DNS cache) can type-assert to this
// interface to share the same database instead of opening separate DB files.
type GORMProvider interface {
	GetGORMDB() *gorm.DB
}
