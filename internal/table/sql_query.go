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
	"fmt"
	"time"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

type SQL struct {
	modName  string
	instName string

	namedArgs bool

	db *gorm.DB

	lookupQuery string
	addQuery    string
	listQuery   string
	setQuery    string
	removeQuery string

	sqliteInMemory     bool
	sqliteSyncInterval time.Duration
}

func NewSQL(modName, instName string, _, _ []string) (module.Module, error) {
	return &SQL{
		modName:  modName,
		instName: instName,
	}, nil
}

func (s *SQL) Name() string {
	return s.modName
}

func (s *SQL) InstanceName() string {
	return s.instName
}

func (s *SQL) Init(cfg *config.Map) error {
	var (
		driver      string
		initQueries []string
		dsnParts    []string
		lookupQuery string

		addQuery    string
		listQuery   string
		removeQuery string
		setQuery    string
	)
	cfg.StringList("init", false, false, nil, &initQueries)
	cfg.String("driver", false, true, "", &driver)
	cfg.StringList("dsn", false, true, nil, &dsnParts)
	cfg.Bool("named_args", false, false, &s.namedArgs)

	cfg.String("lookup", false, true, "", &lookupQuery)

	cfg.String("add", false, false, "", &addQuery)
	cfg.String("list", false, false, "", &listQuery)
	cfg.String("del", false, false, "", &removeQuery)
	cfg.String("set", false, false, "", &setQuery)
	cfg.Bool("sqlite_in_memory", false, false, &s.sqliteInMemory)
	cfg.Duration("sqlite_sync_interval", false, false, 0, &s.sqliteSyncInterval)
	if _, err := cfg.Process(); err != nil {
		return err
	}

	if driver == "postgres" && s.namedArgs {
		return config.NodeErr(cfg.Block, "PostgreSQL driver does not support named_args")
	}

	db, err := mdb.New(mdb.Config{
		Driver:       driver,
		DSN:          dsnParts,
		Debug:        log.DefaultLogger.Debug,
		InMemory:     s.sqliteInMemory,
		SyncInterval: s.sqliteSyncInterval,
	})
	if err != nil {
		return config.NodeErr(cfg.Block, "failed to open db: %v", err)
	}
	s.db = db

	for _, init := range initQueries {
		if err := db.Exec(init).Error; err != nil {
			return config.NodeErr(cfg.Block, "init query failed: %v", err)
		}
	}

	s.lookupQuery = lookupQuery
	s.addQuery = addQuery
	s.listQuery = listQuery
	s.setQuery = setQuery
	s.removeQuery = removeQuery

	return nil
}

func (s *SQL) Close() error {
	sqlDB, err := s.db.DB()
	if err != nil {
		return err
	}
	return sqlDB.Close()
}

func (s *SQL) Lookup(ctx context.Context, val string) (string, bool, error) {
	var results []string
	var err error

	if s.namedArgs {
		err = s.db.WithContext(ctx).Raw(s.lookupQuery, map[string]interface{}{"key": val}).Scan(&results).Error
	} else {
		err = s.db.WithContext(ctx).Raw(s.lookupQuery, val).Scan(&results).Error
	}

	if err != nil {
		return "", false, err
	}
	if len(results) == 0 {
		return "", false, nil
	}
	return results[0], true, nil
}

func (s *SQL) LookupMulti(ctx context.Context, val string) ([]string, error) {
	var repl []string
	var err error

	if s.namedArgs {
		err = s.db.WithContext(ctx).Raw(s.lookupQuery, map[string]interface{}{"key": val}).Scan(&repl).Error
	} else {
		err = s.db.WithContext(ctx).Raw(s.lookupQuery, val).Scan(&repl).Error
	}

	if err != nil {
		return nil, fmt.Errorf("%s; lookup %s: %w", s.modName, val, err)
	}
	return repl, nil
}

func (s *SQL) Keys() ([]string, error) {
	if s.listQuery == "" {
		return nil, fmt.Errorf("%s: table is not mutable (no 'list' query)", s.modName)
	}

	var list []string
	err := s.db.Raw(s.listQuery).Scan(&list).Error
	if err != nil {
		return nil, fmt.Errorf("%s: list: %w", s.modName, err)
	}
	return list, nil
}

func (s *SQL) RemoveKey(k string) error {
	if s.removeQuery == "" {
		return fmt.Errorf("%s: table is not mutable (no 'del' query)", s.modName)
	}

	var err error
	if s.namedArgs {
		err = s.db.Exec(s.removeQuery, map[string]interface{}{"key": k}).Error
	} else {
		err = s.db.Exec(s.removeQuery, k).Error
	}
	if err != nil {
		return fmt.Errorf("%s: del %s: %w", s.modName, k, err)
	}
	return nil
}

func (s *SQL) SetKey(k, v string) error {
	if s.setQuery == "" {
		return fmt.Errorf("%s: table is not mutable (no 'set' query)", s.modName)
	}
	if s.addQuery == "" {
		return fmt.Errorf("%s: table is not mutable (no 'add' query)", s.modName)
	}

	_, exists, err := s.Lookup(context.TODO(), k)
	if err != nil {
		return fmt.Errorf("%s: set %s: %w", s.modName, k, err)
	}

	if s.namedArgs {
		args := map[string]interface{}{"key": k, "value": v}
		if exists {
			err = s.db.Exec(s.setQuery, args).Error
		} else {
			err = s.db.Exec(s.addQuery, args).Error
		}
	} else {
		if exists {
			err = s.db.Exec(s.setQuery, k, v).Error
		} else {
			err = s.db.Exec(s.addQuery, k, v).Error
		}
	}

	if err != nil {
		return fmt.Errorf("%s: set %s: %w", s.modName, k, err)
	}
	return nil
}

func init() {
	module.Register("table.sql_query", NewSQL)
}
