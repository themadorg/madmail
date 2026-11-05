{{/*
Expand the name of the chart.
*/}}
{{- define "madmail.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Fully qualified app name. Truncated to 42 chars so suffixed names
(-headless, -bootstrap, -udp) stay valid and the StatefulSet name stays under
the 52-char limit of its controller-revision-hash label.
*/}}
{{- define "madmail.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 42 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 42 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 42 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{- define "madmail.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "madmail.labels" -}}
helm.sh/chart: {{ include "madmail.chart" . }}
{{ include "madmail.selectorLabels" . }}
app.kubernetes.io/version: {{ include "madmail.imageTag" . | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "madmail.selectorLabels" -}}
app.kubernetes.io/name: {{ include "madmail.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "madmail.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "madmail.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{- define "madmail.imageTag" -}}
{{- default .Chart.AppVersion .Values.image.tag }}
{{- end }}

{{- define "madmail.image" -}}
{{- printf "%s:%s" .Values.image.repository (include "madmail.imageTag" .) }}
{{- end }}

{{/*
Effective TLS mode, mirroring `madmail install`: an empty mode means autocert
for a DNS domain and self_signed for an IP relay. bootstrap.sh always passes
this explicitly so the chart and install never disagree.
*/}}
{{- define "madmail.tlsMode" -}}
{{- if .Values.madmail.tls.mode }}{{ .Values.madmail.tls.mode }}
{{- else if .Values.madmail.domain }}autocert
{{- else }}self_signed
{{- end }}
{{- end }}

{{/*
Effective TURN relay range. hostNetwork keeps madmail's default range.
*/}}
{{- define "madmail.relayPortMin" -}}
{{- if .Values.hostNetwork }}49152{{ else }}{{ int .Values.madmail.turn.relayPortMin }}{{ end }}
{{- end }}
{{- define "madmail.relayPortMax" -}}
{{- if .Values.hostNetwork }}65535{{ else }}{{ int .Values.madmail.turn.relayPortMax }}{{ end }}
{{- end }}

{{/*
Validate values that would otherwise only fail at runtime.
*/}}
{{- define "madmail.validate" -}}
{{- if not .Values.madmail.publicIp }}
{{- fail "madmail.publicIp is required: the public IP clients reach (reserve a static LoadBalancer IP, or the node IP with hostNetwork=true)" }}
{{- end }}
{{- $mode := .Values.madmail.tls.mode }}
{{- if not (has $mode (list "" "autocert" "self_signed" "file")) }}
{{- fail (printf "madmail.tls.mode must be one of \"\", autocert, self_signed, file (got %q)" $mode) }}
{{- end }}
{{- if and (eq $mode "file") (not .Values.madmail.tls.existingSecret) }}
{{- fail "madmail.tls.mode=file requires madmail.tls.existingSecret (a kubernetes.io/tls Secret)" }}
{{- end }}
{{- if and .Values.madmail.tls.existingSecret (ne $mode "file") }}
{{- fail "madmail.tls.existingSecret is only used with madmail.tls.mode=file" }}
{{- end }}
{{- if and (eq (include "madmail.tlsMode" .) "autocert") (not .Values.madmail.domain) }}
{{- fail "madmail.tls.mode=autocert needs madmail.domain (Let's Encrypt for bare IPs is not supported by this chart; use self_signed or file)" }}
{{- end }}
{{- if and .Values.hostNetwork .Values.madmail.metrics.enabled (not .Values.madmail.metrics.existingSecret) }}
{{- fail "madmail.metrics with hostNetwork=true listens on the node's public interfaces and NetworkPolicy does not apply to host-network pods; set madmail.metrics.existingSecret (HTTP Basic credentials)" }}
{{- end }}
{{- if not .Values.hostNetwork }}
{{- $min := int .Values.madmail.turn.relayPortMin }}
{{- $max := int .Values.madmail.turn.relayPortMax }}
{{- if or (lt $min 1024) (gt $max 65535) (gt $min $max) }}
{{- fail (printf "madmail.turn.relayPortMin/Max must satisfy 1024 <= min <= max <= 65535 (got %d-%d)" $min $max) }}
{{- end }}
{{- if gt (sub $max $min) 999 }}
{{- fail (printf "madmail.turn relay range %d-%d has more than 1000 ports; every port becomes a Service port. Narrow it or use hostNetwork=true" $min $max) }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Ports madmail listens on: list of dicts {name, port, protocol}.
Names are <=15 chars (IANA service name rules) so probes can refer to them.
*/}}
{{- define "madmail.ports" -}}
{{- $ports := list
  (dict "name" "smtp" "port" 25 "protocol" "TCP")
  (dict "name" "http" "port" 80 "protocol" "TCP")
  (dict "name" "imap" "port" 143 "protocol" "TCP")
  (dict "name" "https" "port" 443 "protocol" "TCP")
  (dict "name" "submissions" "port" 465 "protocol" "TCP")
  (dict "name" "submission" "port" 587 "protocol" "TCP")
  (dict "name" "imaps" "port" 993 "protocol" "TCP")
  (dict "name" "turn" "port" 3478 "protocol" "UDP")
  (dict "name" "turn-tcp" "port" 3478 "protocol" "TCP")
}}
{{- if .Values.madmail.shadowsocks.enabled }}
{{- $ports = append $ports (dict "name" "shadowsocks" "port" 8388 "protocol" "TCP") }}
{{- end }}
{{- if .Values.madmail.iroh.enabled }}
{{- $ports = append $ports (dict "name" "iroh" "port" 3340 "protocol" "TCP") }}
{{- end }}
{{- if not .Values.hostNetwork }}
{{- range $p := untilStep (int .Values.madmail.turn.relayPortMin) (int (add1 .Values.madmail.turn.relayPortMax)) 1 }}
{{- $ports = append $ports (dict "name" (printf "relay-%d" $p) "port" $p "protocol" "UDP") }}
{{- end }}
{{- end }}
{{- toYaml $ports }}
{{- end }}

{{/*
Volume mounts shared by the bootstrap and madmail containers.
*/}}
{{- define "madmail.volumeMounts" -}}
- name: state
  mountPath: /var/lib/madmail
- name: config
  mountPath: /etc/madmail
{{- if .Values.madmail.tls.existingSecret }}
- name: tls
  mountPath: /etc/madmail/certs
  readOnly: true
{{- end }}
{{- end }}

{{/*
Environment for templates/configmap.yaml bootstrap.sh.
*/}}
{{- define "madmail.bootstrapEnv" -}}
{{- $m := .Values.madmail -}}
- name: MADMAIL_PUBLIC_IP
  value: {{ $m.publicIp | quote }}
- name: MADMAIL_DOMAIN
  value: {{ $m.domain | quote }}
- name: MADMAIL_HOSTNAME
  value: {{ $m.hostname | quote }}
- name: MADMAIL_LANG
  value: {{ $m.lang | quote }}
- name: MADMAIL_TLS_MODE
  value: {{ include "madmail.tlsMode" . | quote }}
- name: MADMAIL_ACME_EMAIL
  value: {{ $m.tls.acmeEmail | quote }}
- name: MADMAIL_ENABLE_IROH
  value: {{ $m.iroh.enabled | quote }}
{{- if not .Values.hostNetwork }}
- name: MADMAIL_RELAY_PORT_MIN
  value: {{ include "madmail.relayPortMin" . | quote }}
- name: MADMAIL_RELAY_PORT_MAX
  value: {{ include "madmail.relayPortMax" . | quote }}
{{- end }}
- name: MADMAIL_ADMIN_WEB_ENABLED
  value: {{ $m.adminWeb.enabled | quote }}
- name: MADMAIL_ADMIN_WEB_PATH
  value: {{ $m.adminWeb.path | quote }}
- name: MADMAIL_METRICS_ENABLED
  value: {{ $m.metrics.enabled | quote }}
- name: MADMAIL_METRICS_PORT
  value: {{ $m.metrics.port | quote }}
{{- with $m.metrics.existingSecret }}
- name: MADMAIL_METRICS_USERNAME
  valueFrom:
    secretKeyRef:
      name: {{ . }}
      key: username
- name: MADMAIL_MONITOR_PASSWORD
  valueFrom:
    secretKeyRef:
      name: {{ . }}
      key: password
{{- end }}
{{- end }}

{{/*
Pod securityContext. Without hostNetwork the pod may bind ports below 1024 as
non-root through the (namespaced, "safe") unprivileged port sysctl. Host
network pods cannot set it, so they run as root.
*/}}
{{- define "madmail.podSecurityContext" -}}
{{- $sc := deepCopy (default dict .Values.podSecurityContext) }}
{{- if .Values.hostNetwork }}
{{- $_ := set $sc "runAsNonRoot" false }}
{{- $_ := set $sc "runAsUser" 0 }}
{{- $_ := set $sc "runAsGroup" 0 }}
{{- else }}
{{- $sysctls := list }}
{{- $found := false }}
{{- range (default list $sc.sysctls) }}
{{- if eq .name "net.ipv4.ip_unprivileged_port_start" }}{{ $found = true }}{{ end }}
{{- $sysctls = append $sysctls . }}
{{- end }}
{{- if not $found }}
{{- $sysctls = append $sysctls (dict "name" "net.ipv4.ip_unprivileged_port_start" "value" "0") }}
{{- end }}
{{- $_ := set $sc "sysctls" $sysctls }}
{{- end }}
{{- toYaml $sc }}
{{- end }}

{{/*
Container securityContext. Root on the host network still needs
NET_BIND_SERVICE for the low ports.
*/}}
{{- define "madmail.securityContext" -}}
{{- $sc := deepCopy (default dict .Values.securityContext) }}
{{- if .Values.hostNetwork }}
{{- $caps := deepCopy (default dict $sc.capabilities) }}
{{- $add := default list $caps.add }}
{{- if not (has "NET_BIND_SERVICE" $add) }}
{{- $add = append $add "NET_BIND_SERVICE" }}
{{- end }}
{{- $_ := set $caps "add" $add }}
{{- $_ := set $sc "capabilities" $caps }}
{{- end }}
{{- toYaml $sc }}
{{- end }}
