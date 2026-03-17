package main

import (
	"fmt"
	"net/http"
	"runtime"

	_ "github.com/themadorg/madmail"
	"github.com/themadorg/madmail/framework/log"
	maddycli "github.com/themadorg/madmail/internal/cli"
	_ "github.com/themadorg/madmail/internal/cli/ctl"
	_ "net/http/pprof"
)

const pprofPort = "6666"

func main() {
	runtime.SetBlockProfileRate(1)
	go func() {
		log.Println(fmt.Sprintf("pprof: listening on http://0.0.0.0:%s", pprofPort))
		log.Println(http.ListenAndServe(fmt.Sprintf(":%s", pprofPort), nil))
	}()
	maddycli.Run()
}
