package resources

import (
	"encoding/json"
	"errors"
	"testing"

)

type cacheReloadAuthMock struct {
	reloadCalls int
	reloadErr   error
}

func (m *cacheReloadAuthMock) AuthPlain(username, password string) error { return nil }
func (m *cacheReloadAuthMock) ListUsers() ([]string, error)               { return nil, nil }
func (m *cacheReloadAuthMock) GetUserPasswordHash(username string) (string, bool, error) {
	return "", false, nil
}
func (m *cacheReloadAuthMock) CreateUser(username, password string) error      { return nil }
func (m *cacheReloadAuthMock) SetUserPassword(username, password string) error { return nil }
func (m *cacheReloadAuthMock) SetUserPasswordHash(username, hash string) error { return nil }
func (m *cacheReloadAuthMock) DeleteUser(username string) error                { return nil }
func (m *cacheReloadAuthMock) IsRegistrationOpen() (bool, error)                { return false, nil }
func (m *cacheReloadAuthMock) SetRegistrationOpen(open bool) error             { return nil }
func (m *cacheReloadAuthMock) IsJitRegistrationEnabled() (bool, error)         { return false, nil }
func (m *cacheReloadAuthMock) SetJitRegistrationEnabled(enabled bool) error    { return nil }
func (m *cacheReloadAuthMock) IsTurnEnabled() (bool, error)                    { return false, nil }
func (m *cacheReloadAuthMock) SetTurnEnabled(enabled bool) error               { return nil }
func (m *cacheReloadAuthMock) IsLoggingDisabled() (bool, error)                { return false, nil }
func (m *cacheReloadAuthMock) SetLoggingDisabled(disabled bool) error          { return nil }
func (m *cacheReloadAuthMock) GetSetting(key string) (string, bool, error) {
	return "", false, nil
}
func (m *cacheReloadAuthMock) SetSetting(key, value string) error { return nil }
func (m *cacheReloadAuthMock) DeleteSetting(key string) error     { return nil }

func (m *cacheReloadAuthMock) ReloadCredentialsCache() error {
	m.reloadCalls++
	return m.reloadErr
}

type cacheReloadQuotaMock struct {
	reloadCalls int
	reloadErr   error
}

func (m *cacheReloadQuotaMock) ReloadQuotaCache() error {
	m.reloadCalls++
	return m.reloadErr
}

func TestCacheReloadHandler_POST_callsReloaders(t *testing.T) {
	auth := &cacheReloadAuthMock{}
	quota := &cacheReloadQuotaMock{}
	h := CacheReloadHandler(CacheReloadDeps{AuthDB: auth, Storage: quota})

	result, status, err := h("POST", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Fatalf("status %d", status)
	}
	if auth.reloadCalls != 1 {
		t.Errorf("credentials reload calls = %d, want 1", auth.reloadCalls)
	}
	if quota.reloadCalls != 1 {
		t.Errorf("quota reload calls = %d, want 1", quota.reloadCalls)
	}
	resp, ok := result.(cacheReloadResponse)
	if !ok {
		t.Fatalf("got %T", result)
	}
	if !resp.CredentialsCacheReloaded || !resp.QuotaCacheReloaded {
		t.Errorf("flags cred=%v quota=%v", resp.CredentialsCacheReloaded, resp.QuotaCacheReloaded)
	}
}

func TestCacheReloadHandler_POST_credentialsError(t *testing.T) {
	auth := &cacheReloadAuthMock{reloadErr: errors.New("boom")}
	h := CacheReloadHandler(CacheReloadDeps{AuthDB: auth, Storage: nil})

	_, status, err := h("POST", nil)
	if err == nil {
		t.Fatal("expected error")
	}
	if status != 500 {
		t.Errorf("status %d", status)
	}
}

func TestCacheReloadHandler_POST_quotaError(t *testing.T) {
	quota := &cacheReloadQuotaMock{reloadErr: errors.New("quota boom")}
	h := CacheReloadHandler(CacheReloadDeps{AuthDB: nil, Storage: quota})

	_, status, err := h("POST", nil)
	if err == nil {
		t.Fatal("expected error")
	}
	if status != 500 {
		t.Errorf("status %d", status)
	}
}

func TestCacheReloadHandler_GET_notAllowed(t *testing.T) {
	h := CacheReloadHandler(CacheReloadDeps{})
	_, status, err := h("GET", nil)
	if err == nil {
		t.Fatal("expected error")
	}
	if status != 405 {
		t.Errorf("status %d", status)
	}
}

func TestCacheReloadHandler_POST_noReloaders(t *testing.T) {
	// Auth without CredentialsCacheReloader; storage without QuotaCacheReloader.
	h := CacheReloadHandler(CacheReloadDeps{
		AuthDB:  &minimalPlainAuth{},
		Storage: struct{}{},
	})

	result, status, err := h("POST", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if status != 200 {
		t.Fatalf("status %d", status)
	}
	resp := result.(cacheReloadResponse)
	if resp.CredentialsCacheReloaded || resp.QuotaCacheReloaded {
		t.Errorf("expected no reloads, got cred=%v quota=%v", resp.CredentialsCacheReloaded, resp.QuotaCacheReloaded)
	}
	if _, err := json.Marshal(result); err != nil {
		t.Errorf("marshal response: %v", err)
	}
}

// minimalPlainAuth implements PlainUserDB but not CredentialsCacheReloader.
type minimalPlainAuth struct{}

func (minimalPlainAuth) AuthPlain(username, password string) error { return nil }
func (minimalPlainAuth) ListUsers() ([]string, error)               { return nil, nil }
func (minimalPlainAuth) GetUserPasswordHash(username string) (string, bool, error) {
	return "", false, nil
}
func (minimalPlainAuth) CreateUser(username, password string) error      { return nil }
func (minimalPlainAuth) SetUserPassword(username, password string) error { return nil }
func (minimalPlainAuth) SetUserPasswordHash(username, hash string) error { return nil }
func (minimalPlainAuth) DeleteUser(username string) error                { return nil }
func (minimalPlainAuth) IsRegistrationOpen() (bool, error)               { return false, nil }
func (minimalPlainAuth) SetRegistrationOpen(open bool) error             { return nil }
func (minimalPlainAuth) IsJitRegistrationEnabled() (bool, error)         { return false, nil }
func (minimalPlainAuth) SetJitRegistrationEnabled(enabled bool) error    { return nil }
func (minimalPlainAuth) IsTurnEnabled() (bool, error)                    { return false, nil }
func (minimalPlainAuth) SetTurnEnabled(enabled bool) error               { return nil }
func (minimalPlainAuth) IsLoggingDisabled() (bool, error)                { return false, nil }
func (minimalPlainAuth) SetLoggingDisabled(disabled bool) error          { return nil }
func (minimalPlainAuth) GetSetting(key string) (string, bool, error) {
	return "", false, nil
}
func (minimalPlainAuth) SetSetting(key, value string) error { return nil }
func (minimalPlainAuth) DeleteSetting(key string) error     { return nil }
