package resources

import (
	"encoding/json"
	"fmt"

	mdb "github.com/themadorg/madmail/internal/db"
	"gorm.io/gorm"
)

// SharesDeps are the dependencies needed by the shares resource handler.
type SharesDeps struct {
	DB *gorm.DB
}

type shareEntry struct {
	Slug string `json:"slug"`
	URL  string `json:"url"`
	Name string `json:"name"`
}

type shareListResponse struct {
	Shares []shareEntry `json:"shares"`
	Total  int          `json:"total"`
}

type createShareRequest struct {
	Slug string `json:"slug"`
	URL  string `json:"url"`
	Name string `json:"name"`
}

type updateShareRequest struct {
	Slug string `json:"slug"`
	URL  string `json:"url,omitempty"`
	Name string `json:"name,omitempty"`
}

type deleteShareRequest struct {
	Slug string `json:"slug"`
}

// SharesHandler creates a handler for /admin/shares.
func SharesHandler(deps SharesDeps) func(string, json.RawMessage) (interface{}, int, error) {
	return func(method string, body json.RawMessage) (interface{}, int, error) {
		if deps.DB == nil {
			return nil, 503, fmt.Errorf("contact sharing is not enabled")
		}

		switch method {
		case "GET":
			var contacts []mdb.Contact
			if err := deps.DB.Find(&contacts).Error; err != nil {
				return nil, 500, fmt.Errorf("failed to list shares: %v", err)
			}
			shares := make([]shareEntry, len(contacts))
			for i, c := range contacts {
				shares[i] = shareEntry{Slug: c.Slug, URL: c.URL, Name: c.Name}
			}
			return shareListResponse{Shares: shares, Total: len(shares)}, 200, nil

		case "POST":
			var req createShareRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Slug == "" || req.URL == "" {
				return nil, 400, fmt.Errorf("slug and url are required")
			}
			contact := mdb.Contact{Slug: req.Slug, URL: req.URL, Name: req.Name}
			if err := deps.DB.Create(&contact).Error; err != nil {
				return nil, 500, fmt.Errorf("failed to create share: %v", err)
			}
			return shareEntry{Slug: req.Slug, URL: req.URL, Name: req.Name}, 201, nil

		case "PUT":
			var req updateShareRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Slug == "" {
				return nil, 400, fmt.Errorf("slug is required")
			}
			updates := map[string]interface{}{}
			if req.URL != "" {
				updates["url"] = req.URL
			}
			if req.Name != "" {
				updates["name"] = req.Name
			}
			if len(updates) == 0 {
				return nil, 400, fmt.Errorf("at least one field (url, name) must be provided")
			}
			result := deps.DB.Model(&mdb.Contact{}).Where("slug = ?", req.Slug).Updates(updates)
			if result.Error != nil {
				return nil, 500, fmt.Errorf("failed to update share: %v", result.Error)
			}
			if result.RowsAffected == 0 {
				return nil, 404, fmt.Errorf("share not found: %s", req.Slug)
			}
			return map[string]string{"updated": req.Slug}, 200, nil

		case "DELETE":
			var req deleteShareRequest
			if err := json.Unmarshal(body, &req); err != nil {
				return nil, 400, fmt.Errorf("invalid request body: %v", err)
			}
			if req.Slug == "" {
				return nil, 400, fmt.Errorf("slug is required")
			}
			result := deps.DB.Where("slug = ?", req.Slug).Delete(&mdb.Contact{})
			if result.Error != nil {
				return nil, 500, fmt.Errorf("failed to delete share: %v", result.Error)
			}
			if result.RowsAffected == 0 {
				return nil, 404, fmt.Errorf("share not found: %s", req.Slug)
			}
			return map[string]string{"deleted": req.Slug}, 200, nil

		default:
			return nil, 405, fmt.Errorf("method %s not allowed", method)
		}
	}
}
