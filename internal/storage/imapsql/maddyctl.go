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
	"context"
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
		quota = mdb.Quota{
			Username:     accountName,
			CreatedAt:    time.Now().Unix(),
			FirstLoginAt: 1,
		}
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

func (store *Storage) PurgeAllIMAPMsgs() error {
	return store.GORMDB.Exec("DELETE FROM msgs").Error
}

func (store *Storage) PurgeReadIMAPMsgs() error {
	store.Log.Debugf("PurgeReadIMAPMsgs: starting purge")
	return store.GORMDB.Transaction(func(tx *gorm.DB) error {
		// 1. Find all blob keys for seen messages and their current reference counts.
		var items []struct {
			Key  string `gorm:"column:extBodyKey"`
			Refs int    `gorm:"column:refs"`
		}
		// Use LEFT JOIN to find messages regardless of whether extKeys entry exists
		err := tx.Table("msgs").
			Select("msgs.extBodyKey, extKeys.refs").
			Joins("LEFT JOIN extKeys ON msgs.extBodyKey = extKeys.id").
			Where("msgs.seen = 1").
			Scan(&items).Error
		if err != nil {
			return err
		}

		store.Log.Debugf("PurgeReadIMAPMsgs: found messages count=%d", len(items))

		if len(items) == 0 {
			return tx.Exec("DELETE FROM msgs WHERE seen = 1").Error
		}

		// 2. Track which keys should be physically deleted.
		seenCounts := make(map[string]int)
		for _, item := range items {
			if item.Key != "" {
				seenCounts[item.Key]++
			}
		}

		var keysToDelete []string
		for key, count := range seenCounts {
			// Find current ref count
			currentRefs := 0
			for _, item := range items {
				if item.Key == key {
					currentRefs = item.Refs
					break
				}
			}

			if currentRefs <= count {
				keysToDelete = append(keysToDelete, key)
			}
		}

		// 3. Decrement refs or delete from extKeys
		for key, count := range seenCounts {
			if contains(keysToDelete, key) {
				if err := tx.Exec("DELETE FROM extKeys WHERE id = ?", key).Error; err != nil {
					return err
				}
			} else {
				if err := tx.Exec("UPDATE extKeys SET refs = refs - ? WHERE id = ?", count, key).Error; err != nil {
					return err
				}
			}
		}

		// 4. Delete the messages
		if err := tx.Exec("DELETE FROM msgs WHERE seen = 1").Error; err != nil {
			return err
		}

		store.Log.Printf("PurgeReadIMAPMsgs: purged seen messages blobsDeleted=%d", len(keysToDelete))

		// 5. Physically delete the files
		if len(keysToDelete) > 0 && store.blobStore != nil {
			if err := store.blobStore.Delete(context.TODO(), keysToDelete); err != nil {
				store.Log.Error("failed to delete blob files during seen purge", err, "keys")
			}
		}

		return nil
	})
}

func contains(slice []string, s string) bool {
	for _, v := range slice {
		if v == s {
			return true
		}
	}
	return false
}

func (store *Storage) PruneUnreadIMAPMsgs(retention time.Duration) error {
	cutoff := time.Now().Add(-retention).Unix()
	return store.GORMDB.Table("msgs").Where("seen = 0 AND date < ?", cutoff).Delete(nil).Error
}

func (store *Storage) GetIMAPAcct(accountName string) (backend.User, error) {
	return store.Back.GetUser(accountName)
}
