package resources

import (
	"encoding/json"
	"fmt"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// ExchangerDeps are the dependencies needed by the exchanger handler.
type ExchangerDeps struct {
	DB *gorm.DB
}

type exchangerEntry struct {
	Name         string `json:"name"`
	URL          string `json:"url"`
	Enabled      bool   `json:"enabled"`
	PollInterval int    `json:"poll_interval"`
	LastPollAt   string `json:"last_poll_at,omitempty"`
}

type exchangerListResponse struct {
	Exchangers []exchangerEntry `json:"exchangers"`
	Total      int              `json:"total"`
}

// ExchangerHandler creates a handler for /admin/exchangers.
func ExchangerHandler(deps ExchangerDeps) func(string, json.RawMessage) (interface{}, int, error) {
	if deps.DB != nil {
		_ = deps.DB.AutoMigrate(&mdb.Exchanger{})
	}

	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if deps.DB == nil {
			return nil, 503, fmt.Errorf("exchanger database not available")
		}

		switch method {
		case "GET":
			var exchangers []mdb.Exchanger
			if err := deps.DB.Find(&exchangers).Error; err != nil {
				return nil, 500, fmt.Errorf("failed to list exchangers: %v", err)
			}
			entries := make([]exchangerEntry, len(exchangers))
			for i, ex := range exchangers {
				lastPoll := ""
				if !ex.LastPollAt.IsZero() {
					lastPoll = ex.LastPollAt.Format("2006-01-02T15:04:05Z")
				}
				entries[i] = exchangerEntry{
					Name:         ex.Name,
					URL:          ex.URL,
					Enabled:      ex.Enabled,
					PollInterval: ex.PollInterval,
					LastPollAt:   lastPoll,
				}
			}
			return exchangerListResponse{Exchangers: entries, Total: len(entries)}, 200, nil

		case "POST":
			var req struct {
				Name         string `json:"name"`
				URL          string `json:"url"`
				PollInterval int    `json:"poll_interval"`
			}
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Name == "" || req.URL == "" {
				return nil, 400, fmt.Errorf("name and url are required")
			}
			if req.PollInterval <= 0 {
				req.PollInterval = 60
			}
			ex := mdb.Exchanger{
				Name:         req.Name,
				URL:          req.URL,
				Enabled:      true,
				PollInterval: req.PollInterval,
			}
			if err := deps.DB.Create(&ex).Error; err != nil {
				return nil, 500, fmt.Errorf("failed to create exchanger: %v", err)
			}
			return exchangerEntry{
				Name:         req.Name,
				URL:          req.URL,
				Enabled:      true,
				PollInterval: req.PollInterval,
			}, 201, nil

		case "PUT":
			var req struct {
				Name         string `json:"name"`
				Enabled      *bool  `json:"enabled,omitempty"`
				URL          string `json:"url,omitempty"`
				PollInterval *int   `json:"poll_interval,omitempty"`
			}
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Name == "" {
				return nil, 400, fmt.Errorf("name is required")
			}
			updates := map[string]interface{}{}
			if req.Enabled != nil {
				updates["enabled"] = *req.Enabled
			}
			if req.URL != "" {
				updates["url"] = req.URL
			}
			if req.PollInterval != nil && *req.PollInterval > 0 {
				updates["poll_interval"] = *req.PollInterval
			}
			if len(updates) == 0 {
				return nil, 400, fmt.Errorf("no fields to update")
			}
			result := deps.DB.Model(&mdb.Exchanger{}).Where("name = ?", req.Name).Updates(updates)
			if result.Error != nil {
				return nil, 500, fmt.Errorf("failed to update exchanger: %v", result.Error)
			}
			if result.RowsAffected == 0 {
				return nil, 404, fmt.Errorf("exchanger not found: %s", req.Name)
			}
			return map[string]string{"updated": req.Name}, 200, nil

		case "DELETE":
			var req struct {
				Name string `json:"name"`
			}
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Name == "" {
				return nil, 400, fmt.Errorf("name is required")
			}
			result := deps.DB.Where("name = ?", req.Name).Delete(&mdb.Exchanger{})
			if result.Error != nil {
				return nil, 500, fmt.Errorf("failed to delete exchanger: %v", result.Error)
			}
			if result.RowsAffected == 0 {
				return nil, 404, fmt.Errorf("exchanger not found: %s", req.Name)
			}
			return map[string]string{"deleted": req.Name}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}
