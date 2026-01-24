package table

import (
	"context"
	"fmt"
	"time"

	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/log"
	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

type GORMTable struct {
	modName  string
	instName string
	db       *gorm.DB
	table    string

	sqliteInMemory     bool
	sqliteSyncInterval time.Duration
}

func NewGORMTable(modName, instName string, _, _ []string) (module.Module, error) {
	return &GORMTable{
		modName:  modName,
		instName: instName,
	}, nil
}

func (g *GORMTable) Name() string {
	return g.modName
}

func (g *GORMTable) InstanceName() string {
	return g.instName
}

func (g *GORMTable) Init(cfg *config.Map) error {
	var (
		driver    string
		dsnParts  []string
		tableName string
	)
	cfg.String("driver", false, true, "", &driver)
	cfg.StringList("dsn", false, true, nil, &dsnParts)
	cfg.String("table_name", false, true, "", &tableName)
	cfg.Bool("sqlite_in_memory", false, false, &g.sqliteInMemory)
	cfg.Duration("sqlite_sync_interval", false, false, 0, &g.sqliteSyncInterval)
	if _, err := cfg.Process(); err != nil {
		return err
	}

	database, err := db.New(db.Config{
		Driver:       driver,
		DSN:          dsnParts,
		Debug:        log.DefaultLogger.Debug,
		InMemory:     g.sqliteInMemory,
		SyncInterval: g.sqliteSyncInterval,
	})
	if err != nil {
		return err
	}
	g.db = database
	g.table = tableName

	// Auto-migrate the table entry model using the configured table name
	if err := g.db.Table(g.table).AutoMigrate(&db.TableEntry{}); err != nil {
		return fmt.Errorf("failed to auto-migrate table %s: %w", g.table, err)
	}

	return nil
}

func (g *GORMTable) Close() error {
	sqlDB, err := g.db.DB()
	if err != nil {
		return err
	}
	return sqlDB.Close()
}

func (g *GORMTable) Lookup(ctx context.Context, key string) (string, bool, error) {
	var entry db.TableEntry
	err := g.db.WithContext(ctx).Table(g.table).Where("\"key\" = ?", key).First(&entry).Error
	if err != nil {
		if err == gorm.ErrRecordNotFound {
			return "", false, nil
		}
		return "", false, err
	}
	return entry.Value, true, nil
}

func (g *GORMTable) LookupMulti(ctx context.Context, key string) ([]string, error) {
	var entries []db.TableEntry
	err := g.db.WithContext(ctx).Table(g.table).Where("\"key\" = ?", key).Find(&entries).Error
	if err != nil {
		return nil, err
	}
	res := make([]string, len(entries))
	for i, e := range entries {
		res[i] = e.Value
	}
	return res, nil
}

func (g *GORMTable) Keys() ([]string, error) {
	var keys []string
	err := g.db.Table(g.table).Pluck("key", &keys).Error
	return keys, err
}

func (g *GORMTable) RemoveKey(k string) error {
	return g.db.Table(g.table).Where("\"key\" = ?", k).Delete(&db.TableEntry{}).Error
}

func (g *GORMTable) SetKey(k, v string) error {
	entry := db.TableEntry{Key: k, Value: v}
	return g.db.Table(g.table).Save(&entry).Error
}

func init() {
	module.Register("table.gorm", NewGORMTable)
}
