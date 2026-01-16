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

package imapsql

import (
	"errors"
	"time"

	"github.com/emersion/go-imap/backend"
	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// These methods wrap corresponding go-imap-sql methods, but also apply
// maddy-specific credentials rules.

func (store *Storage) ListIMAPAccts() ([]string, error) {
	return store.Back.ListUsers()
}

func (store *Storage) CreateIMAPAcct(accountName string) error {
	if err := store.Back.CreateUser(accountName); err != nil {
		return err
	}

	var quota mdb.Quota
	err := store.GORMDB.Where("username = ?", accountName).First(&quota).Error
	if err != nil && !errors.Is(err, gorm.ErrRecordNotFound) {
		return err
	}

	if errors.Is(err, gorm.ErrRecordNotFound) {
		quota = mdb.Quota{Username: accountName, CreatedAt: time.Now().Unix()}
		return store.GORMDB.Create(&quota).Error
	} else if quota.CreatedAt == 0 {
		quota.CreatedAt = time.Now().Unix()
		return store.GORMDB.Save(&quota).Error
	}

	return nil
}

func (store *Storage) DeleteIMAPAcct(accountName string) error {
	store.GORMDB.Where("username = ?", accountName).Delete(&mdb.Quota{})
	return store.Back.DeleteUser(accountName)
}

func (store *Storage) PurgeIMAPMsgs(username string) error {
	return store.GORMDB.Table("msgs").Where("\"mboxId\" IN (SELECT id FROM mboxes WHERE uid IN (SELECT id FROM users WHERE username = ?))", username).Delete(nil).Error
}

func (store *Storage) GetIMAPAcct(accountName string) (backend.User, error) {
	return store.Back.GetUser(accountName)
}
