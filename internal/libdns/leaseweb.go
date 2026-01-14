//go:build libdns_leaseweb || libdns_all
// +build libdns_leaseweb libdns_all

package libdns

import (
	"github.com/libdns/leaseweb"
	"github.com/themadorg/madmail/framework/config"
	"github.com/themadorg/madmail/framework/module"
)

func init() {
	module.Register("libdns.leaseweb", func(modName, instName string, _, _ []string) (module.Module, error) {
		p := leaseweb.Provider{}
		return &ProviderModule{
			RecordDeleter:  &p,
			RecordAppender: &p,
			setConfig: func(c *config.Map) {
				c.String("api_key", false, false, "", &p.APIKey)
			},
			instName: instName,
			modName:  modName,
		}, nil
	})
}
