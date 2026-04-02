package ctl

import (
	"fmt"

	maddycli "github.com/themadorg/madmail/internal/cli"
	"github.com/urfave/cli/v2"
)

func init() {
	maddycli.AddSubcommand(&cli.Command{
		Name:  "language",
		Usage: "View or change the website language",
		Description: `View or change the language used for the web interface.

Supported languages: en (English), fa (Farsi), ru (Russian), es (Spanish).

The change takes effect immediately without a restart.

Examples:
  maddy language                 Show the current language
  maddy language set fa          Switch to Farsi
  maddy language set en          Switch to English
  maddy language reset           Reset to config default`,
		Subcommands: []*cli.Command{
			{
				Name:  "status",
				Usage: "Show the current website language",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: languageStatus,
			},
			{
				Name:      "set",
				Usage:     "Set the website language (en, fa, ru, es)",
				ArgsUsage: "<LANG>",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: languageSet,
			},
			{
				Name:  "reset",
				Usage: "Reset the language to config default",
				Flags: []cli.Flag{
					&cli.StringFlag{
						Name:    "state-dir",
						Usage:   "Path to the state directory",
						EnvVars: []string{"MADDY_STATE_DIR"},
					},
				},
				Action: languageReset,
			},
		},
		// Default action when run without a subcommand: show current language
		Action: languageStatus,
		Flags: []cli.Flag{
			&cli.StringFlag{
				Name:    "state-dir",
				Usage:   "Path to the state directory",
				EnvVars: []string{"MADDY_STATE_DIR"},
			},
		},
	})
}

const dbKeyLanguage = "__LANGUAGE__"

var validLanguages = map[string]string{
	"en": "English",
	"fa": "فارسی (Farsi)",
	"ru": "Русский (Russian)",
	"es": "Español (Spanish)",
}

func languageStatus(c *cli.Context) error {
	cfg := getDBConfig(c)
	settings := readSettingsFromDB(cfg)

	lang := "(config default)"
	if v, ok := settings[dbKeyLanguage]; ok && v != "" {
		name := v
		if full, exists := validLanguages[v]; exists {
			name = fmt.Sprintf("%s — %s", v, full)
		}
		lang = fmt.Sprintf("%s (DB override)", name)
	}

	fmt.Println()
	fmt.Printf("  Website Language:  %s\n", lang)
	fmt.Println()
	return nil
}

func languageSet(c *cli.Context) error {
	lang := c.Args().First()
	if lang == "" {
		return fmt.Errorf("language code is required (en, fa, ru, es)")
	}

	if _, ok := validLanguages[lang]; !ok {
		return fmt.Errorf("unsupported language: %s\nSupported: en, fa, ru, es", lang)
	}

	cfg := getDBConfig(c)
	if err := setSetting(cfg, dbKeyLanguage, lang); err != nil {
		return fmt.Errorf("failed to set language: %v", err)
	}
	fmt.Printf("🌐 Website language set to %s — %s (effective immediately)\n", lang, validLanguages[lang])
	return nil
}

func languageReset(c *cli.Context) error {
	cfg := getDBConfig(c)
	if err := deleteSetting(cfg, dbKeyLanguage); err != nil {
		return fmt.Errorf("failed to reset language: %v", err)
	}
	fmt.Println("🔄 Website language reset to config default (effective immediately)")
	return nil
}
