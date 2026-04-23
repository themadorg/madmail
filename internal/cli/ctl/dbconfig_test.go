package ctl

import "testing"

const sampleConf = `
auth.pass_table local_authdb {
    auto_create yes

    table sql_table {
        driver sqlite3
        dsn credentials.db       # Relative to state_dir
        table_name passwords
    }

    # settings_table sql_table {
    #     driver sqlite3
    #     dsn credentials.db
    #     table_name settings
    # }
}

storage.imapsql local_mailboxes {
    auto_create yes
    driver sqlite3
    dsn imapsql.db               # Relative to state_dir
    retention 24h
    default_quota 1G
}
`

func TestParseAuthPassTableByName(t *testing.T) {
	main, settings := parseAuthPassTableByName(sampleConf, "local_authdb")

	if main.Driver != "sqlite3" {
		t.Errorf("main.Driver = %q, want sqlite3", main.Driver)
	}
	if main.DSN != "credentials.db" {
		t.Errorf("main.DSN = %q, want credentials.db (inline comment should be stripped)", main.DSN)
	}
	if main.TableName != "passwords" {
		t.Errorf("main.TableName = %q, want passwords", main.TableName)
	}
	if settings.Driver != "" {
		t.Errorf("settings should be empty when the settings_table block is commented out, got %+v", settings)
	}
}

func TestParseAuthPassTableByName_WithSettings(t *testing.T) {
	conf := `
auth.pass_table local_authdb {
    table sql_table {
        driver sqlite3
        dsn credentials.db
        table_name passwords
    }
    settings_table sql_table {
        driver sqlite3
        dsn settings.db
        table_name settings
    }
}
`
	main, settings := parseAuthPassTableByName(conf, "local_authdb")

	if main.TableName != "passwords" {
		t.Errorf("main.TableName = %q", main.TableName)
	}
	if settings.Driver != "sqlite3" || settings.DSN != "settings.db" || settings.TableName != "settings" {
		t.Errorf("settings = %+v, want {sqlite3 settings.db settings}", settings)
	}
}

func TestParseAuthPassTableByName_NameMismatch(t *testing.T) {
	main, settings := parseAuthPassTableByName(sampleConf, "other_block")
	if main.Driver != "" || settings.Driver != "" {
		t.Errorf("expected empty configs for unknown block name, got main=%+v settings=%+v", main, settings)
	}
}

func TestParseStorageImapsqlByName(t *testing.T) {
	storage := parseStorageImapsqlByName(sampleConf, "local_mailboxes")
	if storage.Driver != "sqlite3" {
		t.Errorf("Driver = %q", storage.Driver)
	}
	if storage.DSN != "imapsql.db" {
		t.Errorf("DSN = %q, want imapsql.db (inline comment should be stripped)", storage.DSN)
	}
}

func TestParseStorageImapsqlByName_Missing(t *testing.T) {
	storage := parseStorageImapsqlByName(sampleConf, "nope")
	if storage.Driver != "" || storage.DSN != "" {
		t.Errorf("expected empty config for unknown block, got %+v", storage)
	}
}

func TestParseAuthTableKVStore_CommentedSettingsUsesMainTable(t *testing.T) {
	d, s, tbl := parseAuthTableKVStore(sampleConf)
	if d != "sqlite3" || s != "credentials.db" {
		t.Errorf("parseAuthTableKVStore = driver %q dsn %q, want sqlite3 + credentials.db", d, s)
	}
	if tbl != "passwords" {
		t.Errorf("TableName = %q, want passwords (settings table is commented out)", tbl)
	}
}

func TestParseAuthTableKVStore_SettingsTable(t *testing.T) {
	conf := `
auth.pass_table local_authdb {
    table sql_table {
        driver sqlite3
        dsn credentials.db
        table_name passwords
    }
    settings_table sql_table {
        driver sqlite3
        dsn settings.db
        table_name settings
    }
}
`
	d, s, tbl := parseAuthTableKVStore(conf)
	if d != "sqlite3" || s != "settings.db" || tbl != "settings" {
		t.Errorf("parseAuthTableKVStore = %q %q %q, want sqlite3 settings.db settings", d, s, tbl)
	}
}
