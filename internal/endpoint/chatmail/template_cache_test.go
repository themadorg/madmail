package chatmail

import (
	"html/template"
	"testing"
)

func TestGetTemplateCachesByName(t *testing.T) {
	t.Parallel()

	e := &Endpoint{
		tmplCache: map[string]*template.Template{},
	}

	raw := []byte("{{.MailDomain}}")
	tmpl1, err := e.getTemplate("sample.html", raw)
	if err != nil {
		t.Fatalf("getTemplate failed on first call: %v", err)
	}
	tmpl2, err := e.getTemplate("sample.html", raw)
	if err != nil {
		t.Fatalf("getTemplate failed on second call: %v", err)
	}

	if tmpl1 != tmpl2 {
		t.Fatal("expected cached template instance to be reused")
	}
}

func TestGetTemplateDoesNotCacheParseError(t *testing.T) {
	t.Parallel()

	e := &Endpoint{
		tmplCache: map[string]*template.Template{},
	}

	if _, err := e.getTemplate("broken.html", []byte("{{")); err == nil {
		t.Fatal("expected parse error for broken template")
	}
	if _, ok := e.tmplCache["broken.html"]; ok {
		t.Fatal("broken template should not be cached")
	}
}
