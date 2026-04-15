module github.com/foxcpp/go-imap-sql

go 1.12

require (
	github.com/cpuguy83/go-md2man/v2 v2.0.3 // indirect
	github.com/emersion/go-imap v1.2.2-0.20220928192137-6fac715be9cf
	github.com/emersion/go-imap-sortthread v1.2.0
	github.com/emersion/go-message v0.18.0
	github.com/emersion/go-sasl v0.0.0-20231106173351-e73c9f7bad43 // indirect
	github.com/foxcpp/go-imap-backend-tests v0.0.0-20220105184719-e80aa29a5e16
	github.com/foxcpp/go-imap-mess v0.0.0-20230108134257-b7ec3a649613
	github.com/foxcpp/go-imap-namespace v0.0.0-20200802091432-08496dd8e0ed
	github.com/frankban/quicktest v1.5.0 // indirect
	github.com/google/go-cmp v0.5.5 // indirect
	github.com/klauspost/compress v1.17.4
	github.com/lib/pq v1.10.9
	github.com/mailru/easyjson v0.7.7
	github.com/mattn/go-sqlite3 v1.14.19
	github.com/pierrec/lz4 v2.6.1+incompatible
	github.com/urfave/cli v1.22.14
	gotest.tools v2.2.0+incompatible
)

replace github.com/emersion/go-imap => github.com/foxcpp/go-imap v1.0.0-beta.1.0.20220623182312-df940c324887

replace github.com/foxcpp/go-imap-mess => ../go-imap-mess
