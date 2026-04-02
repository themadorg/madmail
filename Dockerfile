FROM oven/bun:1 AS admin-web-build
WORKDIR /app
COPY admin-web/package.json admin-web/bun.lock* ./
RUN bun install --frozen-lockfile
COPY admin-web/ .
RUN bun run build

FROM golang:1.25-alpine AS build-env

ARG ADDITIONAL_BUILD_TAGS=""

RUN set -ex && \
    apk upgrade --no-cache --available && \
    apk add --no-cache build-base curl git tar

WORKDIR /madmail

COPY go.mod go.sum ./
COPY internal/go-imap-sql ./internal/go-imap-sql
COPY internal/go-imap-mess ./internal/go-imap-mess
RUN go mod download

COPY . ./
# Copy built admin-web from previous stage
COPY --from=admin-web-build /app/build ./admin-web/build

RUN mkdir -p internal/endpoint/iroh/assets && \
    touch internal/endpoint/iroh/assets/iroh-relay && \
    mkdir -p /pkg/data && \
    cp maddy.conf.docker /pkg/data/maddy.conf && \
    ./build.sh --builddir /tmp --destdir /pkg/ --tags "docker ${ADDITIONAL_BUILD_TAGS}" build install

FROM alpine:3.21.2
LABEL maintainer="Madmail <admin@madmail.chat>"
LABEL org.opencontainers.image.source=https://github.com/themadorg/madmail

RUN set -ex && \
    apk upgrade --no-cache --available && \
    apk --no-cache add ca-certificates tzdata

COPY --from=build-env /pkg/data/maddy.conf /data/maddy.conf
COPY --from=build-env /pkg/usr/local/bin/maddy /bin/
COPY --from=build-env /madmail/internal/endpoint/iroh/assets/iroh-relay /bin/iroh-relay

EXPOSE 25 143 993 587 465 8080
VOLUME ["/data"]
ENTRYPOINT ["/bin/maddy", "-config", "/data/maddy.conf"]
CMD ["run"]
