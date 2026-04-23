module github.com/foxcpp/go-imap-sql

go 1.21

require (
	github.com/emersion/go-imap v1.2.2-0.20220928192137-6fac715be9cf
	github.com/emersion/go-imap-sortthread v1.2.0
	github.com/emersion/go-message v0.18.0
	github.com/foxcpp/go-imap-backend-tests v0.0.0-20220105184719-e80aa29a5e16
	github.com/foxcpp/go-imap-mess v0.0.0-20230108134257-b7ec3a649613
	github.com/foxcpp/go-imap-namespace v0.0.0-20200802091432-08496dd8e0ed
	github.com/klauspost/compress v1.17.4
	github.com/lib/pq v1.10.9
	github.com/mailru/easyjson v0.7.7
	github.com/pierrec/lz4 v2.6.1+incompatible
	github.com/urfave/cli v1.22.14
	gotest.tools v2.2.0+incompatible
	modernc.org/sqlite v1.34.5
)

require (
	github.com/cpuguy83/go-md2man/v2 v2.0.3 // indirect
	github.com/dustin/go-humanize v1.0.1 // indirect
	github.com/emersion/go-sasl v0.0.0-20231106173351-e73c9f7bad43 // indirect
	github.com/emersion/go-textwrapper v0.0.0-20200911093747-65d896831594 // indirect
	github.com/frankban/quicktest v1.5.0 // indirect
	github.com/google/go-cmp v0.5.9 // indirect
	github.com/google/uuid v1.6.0 // indirect
	github.com/josharian/intern v1.0.0 // indirect
	github.com/mattn/go-isatty v0.0.20 // indirect
	github.com/ncruces/go-strftime v0.1.9 // indirect
	github.com/pkg/errors v0.8.1 // indirect
	github.com/remyoudompheng/bigfft v0.0.0-20230129092748-24d4a6f8daec // indirect
	github.com/russross/blackfriday/v2 v2.1.0 // indirect
	golang.org/x/sys v0.22.0 // indirect
	golang.org/x/text v0.14.0 // indirect
	modernc.org/libc v1.55.3 // indirect
	modernc.org/mathutil v1.6.0 // indirect
	modernc.org/memory v1.8.0 // indirect
)

replace github.com/emersion/go-imap => github.com/foxcpp/go-imap v1.0.0-beta.1.0.20220623182312-df940c324887

replace github.com/foxcpp/go-imap-mess => ../go-imap-mess
