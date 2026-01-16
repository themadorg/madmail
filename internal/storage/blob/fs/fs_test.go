package fs

import (
	"os"
	"testing"

	"github.com/themadorg/madmail/framework/module"
	"github.com/themadorg/madmail/internal/storage/blob"
	"github.com/themadorg/madmail/internal/testutils"
)

func TestFS(t *testing.T) {
	blob.TestStore(t, func() module.BlobStore {
		dir := testutils.Dir(t)
		return &FSStore{instName: "test", root: dir}
	}, func(store module.BlobStore) {
		os.RemoveAll(store.(*FSStore).root)
	})
}
