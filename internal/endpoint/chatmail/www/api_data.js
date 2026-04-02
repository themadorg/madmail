/**
 * API Documentation Data — all endpoint info in en/fa/ru.
 * Update this file when APIs change; the HTML template renders it automatically.
 */
var API_DOCS = {
    title: { en: "Admin API Documentation", fa: "مستندات Admin API", ru: "Документация Admin API" },
    toc_title: { en: "Table of Contents", fa: "فهرست مطالب", ru: "Оглавление" },

    sections: [
        // ── Architecture ──
        {
            id: "architecture",
            title: { en: "Architecture", fa: "معماری و نحوه کار", ru: "Архитектура" },
            paragraphs: [
                { en: "The Admin API uses a <strong>Single-Endpoint RPC</strong> architecture. Instead of multiple HTTP routes, all requests are sent to one address:", fa: "Admin API از یک معماری <strong>RPC تک‌نقطه</strong> (Single-Endpoint RPC) استفاده می‌کند. به جای داشتن مسیرهای HTTP مختلف برای هر عملیات، تمام درخواست‌ها به یک آدرس واحد ارسال می‌شوند:", ru: "Admin API использует архитектуру <strong>Single-Endpoint RPC</strong>. Вместо множества HTTP-маршрутов все запросы отправляются на один адрес:" }
            ],
            diagram: "Client → POST /api/admin → JSON Request → Auth Check → Resource Handler → JSON Response",
            notes: [
                { type: "info", title: { en: "Why single endpoint?", fa: "چرا تک‌نقطه؟", ru: "Почему один эндпоинт?" }, text: { en: "This design allows better API concealment. Only one HTTP path is visible from outside, and all responses (even errors) return HTTP 200. The real operation status is only visible in the JSON body.", fa: "این طراحی امکان مخفی‌سازی بهتر API را فراهم می‌کند. از بیرون، تنها یک مسیر HTTP قابل مشاهده است و تمام پاسخ‌ها (حتی خطاها) با کد HTTP 200 بازگردانده می‌شوند. وضعیت واقعی عملیات فقط در بدنه JSON قابل مشاهده است.", ru: "Такая архитектура обеспечивает лучшую маскировку API. Извне виден только один HTTP-путь, и все ответы (даже ошибки) возвращают HTTP 200. Реальный статус операции виден только в теле JSON." } }
            ]
        },
        // ── Authentication ──
        {
            id: "auth",
            title: { en: "Authentication & Token", fa: "احراز هویت و توکن", ru: "Аутентификация и токен" },
            subsections: [
                {
                    title: { en: "Automatic Token", fa: "توکن خودکار", ru: "Автоматический токен" },
                    text: { en: "The admin token is automatically generated on first server run and saved to <code>/var/lib/maddy/admin_token</code>. It contains 256 bits of cryptographic entropy and is preserved across restarts.", fa: "توکن مدیریتی به صورت خودکار در اولین اجرای سرور تولید و در مسیر <code>/var/lib/maddy/admin_token</code> ذخیره می‌شود. این توکن شامل ۲۵۶ بیت آنتروپی رمزنگاری‌شده است و در هر بار راه‌اندازی مجدد حفظ می‌شود.", ru: "Административный токен автоматически генерируется при первом запуске сервера и сохраняется в <code>/var/lib/maddy/admin_token</code>. Он содержит 256 бит криптографической энтропии и сохраняется при перезапусках." }
                },
                {
                    title: { en: "Get token via CLI", fa: "دریافت توکن با CLI", ru: "Получение токена через CLI" },
                    code: "# Show current token\nmaddy admin-token\n\n# Save to variable\nTOKEN=$(maddy admin-token)"
                },
                {
                    title: { en: "Custom Token Settings", fa: "تنظیمات توکن سفارشی", ru: "Пользовательские настройки токена" },
                    text: { en: "You can set one of the following in your <code>maddy.conf</code>:", fa: "شما می‌توانید در فایل <code>maddy.conf</code> یکی از تنظیمات زیر را اعمال کنید:", ru: "Вы можете указать одно из следующих в вашем <code>maddy.conf</code>:" },
                    code: "# Set custom token\nadmin_token your-custom-secret-token\n\n# Disable API entirely\nadmin_token disabled"
                }
            ],
            notes: [
                { type: "security", title: { en: "🔒 Token Security", fa: "🔒 امنیت توکن", ru: "🔒 Безопасность токена" }, text: { en: "The token file is stored with <code>0600</code> permissions (root-only). The token is never logged and comparison is done in <em>constant-time</em>.", fa: "فایل توکن با مجوز <code>0600</code> (فقط قابل خواندن توسط root) ذخیره می‌شود. توکن هیچ‌گاه در لاگ‌ها ثبت نمی‌شود و مقایسه آن به صورت <em>ثابت‌زمان</em> (constant-time) انجام می‌شود.", ru: "Файл токена хранится с правами <code>0600</code> (только root). Токен никогда не логируется, а сравнение выполняется за <em>постоянное время</em>." } }
            ]
        },
        // ── Request Format ──
        {
            id: "request-format",
            title: { en: "Request & Response Format", fa: "ساختار درخواست و پاسخ", ru: "Формат запроса и ответа" },
            subsections: [
                {
                    title: { en: "Request", fa: "درخواست", ru: "Запрос" },
                    text: { en: "All requests must be <code>POST</code> to <code>/api/admin</code>:", fa: "تمام درخواست‌ها باید به صورت <code>POST</code> به آدرس <code>/api/admin</code> ارسال شوند:", ru: "Все запросы должны быть <code>POST</code> на <code>/api/admin</code>:" },
                    code: '{\n    "method":   "GET",\n    "resource": "/admin/status",\n    "headers":  {\n        "Authorization": "Bearer YOUR_TOKEN"\n    },\n    "body":     {}\n}',
                    fields: [
                        { name: "method", desc: { en: "Operation method: GET, POST, PUT, DELETE", fa: "متد عملیات: GET، POST، PUT، DELETE", ru: "Метод операции: GET, POST, PUT, DELETE" } },
                        { name: "resource", desc: { en: "Target resource path", fa: "مسیر منبع مورد نظر", ru: "Путь к целевому ресурсу" } },
                        { name: "headers", desc: { en: "Internal headers (including auth token)", fa: "هدرهای داخلی (شامل توکن احراز هویت)", ru: "Внутренние заголовки (включая токен авторизации)" } },
                        { name: "body", desc: { en: "Optional body (depends on operation)", fa: "بدنه اختیاری (بسته به عملیات)", ru: "Необязательное тело (зависит от операции)" } }
                    ]
                },
                {
                    title: { en: "Response", fa: "پاسخ", ru: "Ответ" },
                    text: { en: "All responses are returned in a uniform format:", fa: "تمام پاسخ‌ها با فرمت یکسان بازگردانده می‌شوند:", ru: "Все ответы возвращаются в едином формате:" },
                    code: '{\n    "status":   200,\n    "resource": "/admin/status",\n    "body":     { ... },\n    "error":    null\n}',
                    fields: [
                        { name: "status", desc: { en: "Actual operation status code", fa: "کد وضعیت واقعی عملیات", ru: "Реальный код статуса операции" } },
                        { name: "resource", desc: { en: "Requested resource path", fa: "مسیر منبع درخواست‌شده", ru: "Запрошенный путь ресурса" } },
                        { name: "body", desc: { en: "Returned data (on success)", fa: "داده‌های بازگشتی (در صورت موفقیت)", ru: "Возвращённые данные (при успехе)" } },
                        { name: "error", desc: { en: "Error message (on failure, otherwise null)", fa: "پیام خطا (در صورت شکست، در غیر اینصورت null)", ru: "Сообщение об ошибке (при неудаче, иначе null)" } }
                    ]
                }
            ],
            notes: [
                { type: "warning", title: { en: "⚠️ Note", fa: "⚠️ توجه", ru: "⚠️ Примечание" }, text: { en: "The external HTTP code is always <code>200</code>. To check the actual result, inspect the <code>status</code> field in the JSON body.", fa: "کد HTTP خارجی همیشه <code>200</code> است. برای بررسی نتیجه واقعی، فیلد <code>status</code> در بدنه JSON را بررسی کنید.", ru: "Внешний HTTP-код всегда <code>200</code>. Для проверки реального результата используйте поле <code>status</code> в теле JSON." } }
            ]
        },
        // ── Quick Start ──
        {
            id: "quick-start",
            title: { en: "Quick Start", fa: "شروع سریع", ru: "Быстрый старт" },
            subsections: [
                {
                    title: { en: "curl example", fa: "نمونه با curl", ru: "Пример с curl" },
                    code: '# Get token\nTOKEN=$(maddy admin-token)\n\n# Check server status\ncurl -s -X POST https://your-server/api/admin \\\n  -H \'Content-Type: application/json\' \\\n  -d "{\n    \\"method\\": \\"GET\\",\n    \\"resource\\": \\"/admin/status\\",\n    \\"headers\\": {\\"Authorization\\": \\"Bearer $TOKEN\\"}\n  }" | python3 -m json.tool'
                },
                {
                    title: { en: "Python example", fa: "نمونه پایتون", ru: "Пример на Python" },
                    code: 'import requests\n\nTOKEN = "your-admin-token"\nBASE  = "https://your-server"\n\ndef api(resource, method="GET", body=None):\n    resp = requests.post(f"{BASE}/api/admin", json={\n        "method": method,\n        "resource": resource,\n        "headers": {"Authorization": f"Bearer {TOKEN}"},\n        "body": body or {},\n    })\n    return resp.json()\n\n# Example: get status\nprint(api("/admin/status"))'
                }
            ]
        }
    ],

    // ── Endpoints ──
    endpoints: [
        {
            id: "status", resource: "/admin/status", methods: ["GET"],
            title: { en: "Server Status", fa: "وضعیت سرور", ru: "Статус сервера" },
            desc: { en: "Display overall server status including user count, uptime, and email server stats.", fa: "نمایش وضعیت کلی سرور شامل تعداد کاربران، آپتایم و آمار سرورهای ایمیل.", ru: "Отображение общего статуса сервера, включая количество пользователей, аптайм и статистику email-серверов." },
            request: '{\n    "method": "GET",\n    "resource": "/admin/status",\n    "headers": {"Authorization": "Bearer TOKEN"}\n}',
            response: '{\n    "status": 200,\n    "body": {\n        "users": {\n            "registered": 799\n        },\n        "uptime": {\n            "boot_time": "2026-02-17T20:51:21Z",\n            "duration": "2d 5h 30m 15s"\n        },\n        "email_servers": {\n            "connection_ips": 12,\n            "domain_servers": 8,\n            "ip_servers": 3\n        }\n    }\n}',
            fields: [
                { name: "users.registered", type: "int", desc: "Total registered accounts" },
                { name: "uptime.boot_time", type: "string", desc: "Server boot time (RFC3339)" },
                { name: "uptime.duration", type: "string", desc: "Human-readable uptime" },
                { name: "email_servers.connection_ips", type: "int", desc: "Unique connecting IPs since boot" },
                { name: "email_servers.domain_servers", type: "int", desc: "Unique domain servers seen" },
                { name: "email_servers.ip_servers", type: "int", desc: "Unique IP-based servers seen" }
            ]
        },
        {
            id: "storage", resource: "/admin/storage", methods: ["GET"],
            title: { en: "Storage Info", fa: "اطلاعات فضای ذخیره‌سازی", ru: "Информация о хранилище" },
            desc: { en: "Disk info, data directory and database size.", fa: "اطلاعات دیسک، پوشه داده و حجم دیتابیس.", ru: "Информация о диске, каталоге данных и размере базы данных." },
            response: '{\n    "status": 200,\n    "body": {\n        "disk": {\n            "total_bytes": 53687091200,\n            "used_bytes": 18253611008,\n            "available_bytes": 35433480192,\n            "percent_used": 34.0\n        },\n        "state_dir": {\n            "path": "/var/lib/maddy",\n            "size_bytes": 1073741824\n        },\n        "database": {\n            "driver": "sqlite3",\n            "size_bytes": 52428800\n        }\n    }\n}'
        },
        {
            id: "registration", resource: "/admin/registration", methods: ["GET", "POST"],
            title: { en: "Registration Management", fa: "مدیریت ثبت‌نام", ru: "Управление регистрацией" },
            desc: { en: "Open and close user registration.", fa: "باز و بسته کردن ثبت‌نام کاربران جدید.", ru: "Открытие и закрытие регистрации пользователей." },
            examples: [
                { label: { en: "View status", fa: "مشاهده وضعیت", ru: "Просмотр статуса" }, code: '// Request\n{"method": "GET", "resource": "/admin/registration", ...}\n\n// Response\n{"status": 200, "body": {"status": "open"}}' },
                { label: { en: "Change status", fa: "تغییر وضعیت", ru: "Изменить статус" }, code: '// Close registration\n{"method": "POST", "resource": "/admin/registration",\n "body": {"action": "close"}, ...}\n\n// Open registration\n{"method": "POST", "resource": "/admin/registration",\n "body": {"action": "open"}, ...}' }
            ],
            action_table: [
                { action: "open", desc: { en: "Allow new user registration", fa: "اجازه ثبت‌نام کاربران جدید", ru: "Разрешить регистрацию новых пользователей" } },
                { action: "close", desc: { en: "Block new registrations, existing users unaffected", fa: "مسدود کردن ثبت‌نام، کاربران فعلی تأثیر نمی‌بینند", ru: "Блокировать новые регистрации, существующие пользователи не затрагиваются" } }
            ]
        },
        {
            id: "jit", resource: "/admin/registration/jit", methods: ["GET", "POST"],
            title: { en: "JIT Registration", fa: "ثبت‌نام آنی (JIT)", ru: "JIT-регистрация" },
            desc: { en: "Enable/disable Just-In-Time registration. When enabled, user accounts are automatically created on first login.", fa: "فعال/غیرفعال کردن ثبت‌نام آنی. وقتی فعال باشد، حساب کاربری در اولین لاگین به‌صورت خودکار ساخته می‌شود.", ru: "Включение/отключение JIT-регистрации. При включении учётные записи создаются автоматически при первом входе." },
            examples: [
                { label: { en: "Enable/Disable", fa: "فعال/غیرفعال", ru: "Вкл/Выкл" }, code: '// Enable\n{"method": "POST", "resource": "/admin/registration/jit",\n "body": {"action": "enable"}, ...}\n\n// Disable\n{"method": "POST", "resource": "/admin/registration/jit",\n "body": {"action": "disable"}, ...}\n\n// Response\n{"status": 200, "body": {"status": "enabled"}}' }
            ]
        },
        {
            id: "turn", resource: "/admin/services/turn", methods: ["GET", "POST"],
            title: { en: "TURN Service", fa: "سرویس تماس (TURN)", ru: "Сервис TURN" },
            desc: { en: "Enable/disable the TURN server for voice and video calls.", fa: "فعال/غیرفعال کردن سرور TURN برای تماس‌های صوتی و تصویری.", ru: "Включение/отключение TURN-сервера для голосовых и видеозвонков." },
            toggle_code: '/admin/services/turn'
        },
        {
            id: "iroh", resource: "/admin/services/iroh", methods: ["GET", "POST"],
            title: { en: "Iroh Service", fa: "سرویس Iroh", ru: "Сервис Iroh" },
            desc: { en: "Enable/disable the Iroh relay server for real-time Webxdc connections.", fa: "فعال/غیرفعال کردن سرور رله Iroh برای اتصالات Webxdc بلادرنگ.", ru: "Включение/отключение Iroh-реле для подключений Webxdc в реальном времени." },
            toggle_code: '/admin/services/iroh'
        },
        {
            id: "shadowsocks", resource: "/admin/services/shadowsocks", methods: ["GET", "POST"],
            title: { en: "Shadowsocks Service", fa: "سرویس Shadowsocks", ru: "Сервис Shadowsocks" },
            desc: { en: "Enable/disable the Shadowsocks proxy. Used for censorship circumvention.", fa: "فعال/غیرفعال کردن پروکسی Shadowsocks. این سرویس برای عبور از سانسور استفاده می‌شود.", ru: "Включение/отключение прокси Shadowsocks. Используется для обхода цензуры." },
            toggle_code: '/admin/services/shadowsocks'
        },
        {
            id: "log", resource: "/admin/services/log", methods: ["GET", "POST"],
            title: { en: "Log Management", fa: "مدیریت لاگ", ru: "Управление логами" },
            desc: { en: "Enable/disable server logging (no-log policy).", fa: "فعال/غیرفعال کردن ثبت لاگ سرور (سیاست بدون لاگ).", ru: "Включение/отключение серверного логирования (политика без логов)." },
            toggle_code: '/admin/services/log'
        },
        {
            id: "settings", resource: "/admin/settings", methods: ["GET"],
            title: { en: "All Settings", fa: "تنظیمات یکجا", ru: "Все настройки" },
            desc: { en: "Retrieve all server settings in a single request. Includes toggle keys, ports, and configuration settings.", fa: "دریافت تمام تنظیمات سرور در یک درخواست واحد. شامل کلیدهای روشن/خاموش، پورت‌ها و تنظیمات پیکربندی.", ru: "Получение всех настроек сервера в одном запросе. Включает переключатели, порты и параметры конфигурации." },
            response: '// Request\n{"method": "GET", "resource": "/admin/settings", ...}\n\n// Response (partial)\n{\n    "status": 200,\n    "body": {\n        "registration": "closed",\n        "turn_enabled": "enabled",\n        "iroh_enabled": "enabled",\n        "ss_enabled": "enabled",\n        "smtp_port": {"key": "__SMTP_PORT__", "value": "2525", "is_set": true},\n        "turn_secret": {"key": "__TURN_SECRET__", "value": "", "is_set": false}\n    }\n}'
        },
        {
            id: "language", resource: "/admin/settings/language", methods: ["GET", "POST"],
            title: { en: "Website Language", fa: "زبان وب‌سایت", ru: "Язык сайта" },
            desc: { en: "View or change the website language. Supported languages: <code>en</code> (English), <code>fa</code> (Farsi), <code>ru</code> (Russian), <code>es</code> (Spanish). Changes take effect immediately without a restart.", fa: "مشاهده یا تغییر زبان وب‌سایت. زبان‌های پشتیبانی‌شده: <code>en</code> (انگلیسی)، <code>fa</code> (فارسی)، <code>ru</code> (روسی)، <code>es</code> (اسپانیایی). تغییرات بلافاصله اعمال می‌شوند و نیازی به ریستارت نیست.", ru: "Просмотр или изменение языка сайта. Поддерживаемые языки: <code>en</code> (английский), <code>fa</code> (фарси), <code>ru</code> (русский), <code>es</code> (испанский). Изменения вступают в силу немедленно без перезапуска." },
            examples: [
                { label: { en: "View current language", fa: "مشاهده زبان فعلی", ru: "Просмотр текущего языка" }, code: '// Request\n{"method": "GET", "resource": "/admin/settings/language", ...}\n\n// Response\n{"status": 200, "body": {"key": "__LANGUAGE__", "value": "fa", "is_set": true}}' },
                { label: { en: "Set language", fa: "تنظیم زبان", ru: "Установить язык" }, code: '{"method": "POST", "resource": "/admin/settings/language",\n "body": {"action": "set", "value": "fa"}, ...}\n\n// Reset to config default\n{"method": "POST", "resource": "/admin/settings/language",\n "body": {"action": "reset"}, ...}' },
                { label: { en: "CLI equivalent", fa: "معادل خط فرمان", ru: "Аналог в CLI" }, code: '# View current language\nmaddy language\n\n# Set language\nmaddy language set fa\n\n# Reset to default\nmaddy language reset' }
            ]
        },

        {
            id: "port-settings", resource: "/admin/settings/{port}", methods: ["GET", "POST"],
            title: { en: "Port Settings", fa: "تنظیمات پورت‌ها", ru: "Настройки портов" },
            desc: { en: "Each service port is configurable via a dedicated path. Values are stored in the database and override config file values.", fa: "هر پورت سرویسی از طریق یک مسیر اختصاصی قابل تنظیم است. مقادیر در دیتابیس ذخیره می‌شوند و بر مقادیر فایل پیکربندی اولویت دارند.", ru: "Каждый порт сервиса настраивается через отдельный путь. Значения хранятся в БД и имеют приоритет над конфигурационным файлом." },
            port_table: [
                { endpoint: "/admin/settings/smtp_port", desc: "SMTP server port" },
                { endpoint: "/admin/settings/submission_port", desc: "Submission server port" },
                { endpoint: "/admin/settings/imap_port", desc: "IMAP server port" },
                { endpoint: "/admin/settings/turn_port", desc: "TURN relay port" },
                { endpoint: "/admin/settings/dovecot_port", desc: "Dovecot SASL port" },
                { endpoint: "/admin/settings/iroh_port", desc: "Iroh relay port" },
                { endpoint: "/admin/settings/ss_port", desc: "Shadowsocks proxy port" }
            ],
            examples: [
                { label: { en: "Set", fa: "تنظیم", ru: "Установить" }, code: '{"method": "POST", "resource": "/admin/settings/smtp_port",\n "body": {"action": "set", "value": "2525"}, ...}\n\n// Response\n{"key": "__SMTP_PORT__", "value": "2525", "is_set": true}' },
                { label: { en: "Reset", fa: "بازنشانی", ru: "Сбросить" }, code: '{"method": "POST", "resource": "/admin/settings/smtp_port",\n "body": {"action": "reset"}, ...}' }
            ]
        },
        {
            id: "config-settings", resource: "/admin/settings/{config}", methods: ["GET", "POST"],
            title: { en: "Configuration Settings", fa: "تنظیمات پیکربندی", ru: "Настройки конфигурации" },
            desc: { en: "Service configuration settings like hostname, secret, and URL are managed via the same set/reset pattern.", fa: "تنظیمات پیکربندی سرویس‌ها مانند hostname، secret و URL از طریق همان الگوی set/reset قابل مدیریت هستند.", ru: "Параметры конфигурации сервисов (hostname, secret, URL) управляются через тот же паттерн set/reset." },
            port_table: [
                { endpoint: "/admin/settings/smtp_hostname", desc: "SMTP server hostname" },
                { endpoint: "/admin/settings/turn_realm", desc: "TURN server realm" },
                { endpoint: "/admin/settings/turn_secret", desc: "TURN shared secret" },
                { endpoint: "/admin/settings/turn_relay_ip", desc: "TURN relay IP address" },
                { endpoint: "/admin/settings/turn_ttl", desc: "TURN credential TTL (seconds)" },
                { endpoint: "/admin/settings/iroh_relay_url", desc: "Iroh relay URL" },
                { endpoint: "/admin/settings/ss_cipher", desc: "Shadowsocks cipher algorithm" },
                { endpoint: "/admin/settings/ss_password", desc: "Shadowsocks password" }
            ],
            examples: [
                { label: { en: "Set TURN secret", fa: "تنظیم TURN secret", ru: "Установить TURN secret" }, code: '{"method": "POST", "resource": "/admin/settings/turn_secret",\n "body": {"action": "set", "value": "my-shared-secret"}, ...}' },
                { label: { en: "Read Iroh relay URL", fa: "خواندن Iroh relay URL", ru: "Прочитать Iroh relay URL" }, code: '// Request\n{"method": "GET", "resource": "/admin/settings/iroh_relay_url", ...}\n\n// Response\n{"key": "__IROH_RELAY_URL__", "value": "https://iroh.example.com", "is_set": true}' }
            ],
            notes: [
                { type: "info", title: { en: "💡 Note", fa: "💡 نکته", ru: "💡 Примечание" }, text: { en: "When a setting has <code>is_set: false</code>, it means the default from the config file is being used. Use <code>action: \"reset\"</code> to revert any setting to default.", fa: "وقتی یک تنظیم <code>is_set: false</code> باشد، به معنی استفاده از مقدار پیش‌فرض فایل پیکربندی است. با <code>action: \"reset\"</code> می‌توانید هر تنظیمی را به مقدار پیش‌فرض بازگردانید.", ru: "Когда настройка имеет <code>is_set: false</code>, используется значение по умолчанию из конфигурационного файла. Используйте <code>action: \"reset\"</code> для сброса любой настройки к значению по умолчанию." } }
            ]
        },
        {
            id: "accounts", resource: "/admin/accounts", methods: ["GET", "DELETE"],
            title: { en: "Account Management", fa: "مدیریت حساب‌ها", ru: "Управление аккаунтами" },
            desc: { en: "List and delete user accounts. Account creation via API is not possible (passwords are never transmitted via API).", fa: "لیست و حذف حساب‌های کاربری. ایجاد حساب از طریق API امکان‌پذیر نیست (رمز عبورها هیچ‌گاه از طریق API منتقل نمی‌شوند).", ru: "Список и удаление учётных записей. Создание аккаунтов через API невозможно (пароли никогда не передаются через API)." },
            examples: [
                { label: { en: "List all accounts", fa: "لیست تمام حساب‌ها", ru: "Список всех аккаунтов" }, code: '// Request\n{"method": "GET", "resource": "/admin/accounts", ...}\n\n// Response\n{\n    "status": 200,\n    "body": {\n        "total": 3,\n        "accounts": [\n            {"username": "alice@example.com"},\n            {"username": "bob@example.com"},\n            {"username": "charlie@example.com"}\n        ]\n    }\n}' },
                { label: { en: "Delete an account", fa: "حذف یک حساب", ru: "Удалить аккаунт" }, code: '// Request\n{"method": "DELETE", "resource": "/admin/accounts",\n "body": {"username": "alice@example.com"}, ...}\n\n// Response\n{"status": 200, "body": {"deleted": "alice@example.com"}}' }
            ],
            notes: [
                { type: "danger", title: { en: "⚠️ Warning", fa: "⚠️ هشدار", ru: "⚠️ Предупреждение" }, text: { en: "Account deletion is irreversible. All emails, settings and user data will be permanently deleted, and the username will be permanently blocked.", fa: "حذف حساب غیرقابل بازگشت است. تمام ایمیل‌ها، تنظیمات و اطلاعات کاربر به‌طور کامل حذف خواهند شد و نام کاربری برای همیشه مسدود می‌شود.", ru: "Удаление аккаунта необратимо. Все письма, настройки и данные пользователя будут полностью удалены, а имя пользователя навсегда заблокировано." } }
            ]
        },
        {
            id: "blocklist", resource: "/admin/blocklist", methods: ["GET", "POST", "DELETE"],
            title: { en: "Blocklist", fa: "لیست مسدودی", ru: "Чёрный список" },
            desc: { en: "Manage blocked usernames. Blocked users cannot register via <code>/new</code> or JIT. Deleted accounts are automatically added to this list.", fa: "مدیریت لیست نام‌های کاربری مسدود. کاربران مسدود نمی‌توانند از طریق <code>/new</code> یا JIT ثبت‌نام کنند. حساب‌های حذف‌شده به‌طور خودکار به این لیست اضافه می‌شوند.", ru: "Управление заблокированными именами пользователей. Заблокированные пользователи не могут зарегистрироваться через <code>/new</code> или JIT. Удалённые аккаунты автоматически добавляются в этот список." },
            examples: [
                { label: { en: "List blocked users", fa: "لیست کاربران مسدود", ru: "Список заблокированных" }, code: '// Request\n{"method": "GET", "resource": "/admin/blocklist", ...}\n\n// Response\n{\n    "status": 200,\n    "body": {\n        "total": 2,\n        "blocked": [\n            {\n                "username": "spammer@example.com",\n                "reason": "deleted via admin panel",\n                "blocked_at": "2026-02-18T15:30:00Z"\n            }\n        ]\n    }\n}' },
                { label: { en: "Block a user", fa: "مسدود کردن یک کاربر", ru: "Заблокировать пользователя" }, code: '{"method": "POST", "resource": "/admin/blocklist",\n "body": {"username": "spammer@example.com", "reason": "abuse"}, ...}' },
                { label: { en: "Unblock", fa: "رفع مسدودی", ru: "Разблокировать" }, code: '{"method": "DELETE", "resource": "/admin/blocklist",\n "body": {"username": "spammer@example.com"}, ...}' }
            ],
            fields: [
                { name: "username", type: "string", desc: "Blocked username (required)" },
                { name: "reason", type: "string", desc: "Reason for blocking (optional, default: \"manually blocked\")" },
                { name: "blocked_at", type: "string", desc: "RFC3339 timestamp (in GET responses only)" }
            ],
            notes: [
                { type: "info", title: { en: "💡 Note", fa: "💡 نکته", ru: "💡 Примечание" }, text: { en: "When an account is deleted via <code>/admin/accounts</code>, the username is automatically added to the blocklist. To allow re-registration, first remove the user from the blocklist.", fa: "وقتی یک حساب از طریق <code>/admin/accounts</code> حذف می‌شود، نام کاربری به‌طور خودکار به لیست مسدودی اضافه می‌شود. برای اجازه ثبت‌نام مجدد، ابتدا باید کاربر را از لیست مسدودی حذف کنید.", ru: "При удалении аккаунта через <code>/admin/accounts</code> имя пользователя автоматически добавляется в чёрный список. Для повторной регистрации сначала удалите пользователя из чёрного списка." } }
            ]
        },
        {
            id: "quota", resource: "/admin/quota", methods: ["GET", "PUT", "DELETE"],
            title: { en: "Quota Management", fa: "مدیریت سهمیه", ru: "Управление квотами" },
            desc: { en: "Manage user storage quotas.", fa: "مدیریت سهمیه فضای ذخیره‌سازی کاربران.", ru: "Управление квотами хранилища пользователей." },
            examples: [
                { label: { en: "Overall stats", fa: "آمار کلی", ru: "Общая статистика" }, code: '{\n    "status": 200,\n    "body": {\n        "total_storage_bytes": 1073741824,\n        "accounts_count": 42,\n        "default_quota_bytes": 107374182400\n    }\n}' },
                { label: { en: "User quota", fa: "سهمیه یک کاربر", ru: "Квота пользователя" }, code: '// Request\n{"method": "GET", "resource": "/admin/quota",\n "body": {"username": "alice@example.com"}, ...}\n\n// Response\n{\n    "body": {\n        "username": "alice@example.com",\n        "used_bytes": 52428800,\n        "max_bytes": 107374182400,\n        "is_default": true\n    }\n}' },
                { label: { en: "Set custom quota", fa: "تنظیم سهمیه اختصاصی", ru: "Установить пользовательскую квоту" }, code: '// Set quota for one user (1GB)\n{"method": "PUT", "resource": "/admin/quota",\n "body": {"username": "alice@example.com", "max_bytes": 1073741824}, ...}\n\n// Set default quota for all users (2GB)\n{"method": "PUT", "resource": "/admin/quota",\n "body": {"max_bytes": 2147483648}, ...}' },
                { label: { en: "Reset to default", fa: "بازنشانی به پیش‌فرض", ru: "Сброс к умолчанию" }, code: '{"method": "DELETE", "resource": "/admin/quota",\n "body": {"username": "alice@example.com"}, ...}' }
            ]
        },
        {
            id: "queue", resource: "/admin/queue", methods: ["POST"],
            title: { en: "Queue Operations", fa: "عملیات صف", ru: "Операции с очередью" },
            desc: { en: "Purge stored messages. Only <code>POST</code> method is supported.", fa: "پاکسازی پیام‌های ذخیره‌شده. فقط متد <code>POST</code> پشتیبانی می‌شود.", ru: "Очистка сохранённых сообщений. Поддерживается только метод <code>POST</code>." },
            action_table: [
                { action: "purge_user", desc: { en: "Delete all messages for a specific user (requires <code>username</code>)", fa: "حذف تمام پیام‌های یک کاربر خاص (نیاز به <code>username</code>)", ru: "Удалить все сообщения конкретного пользователя (требуется <code>username</code>)" } },
                { action: "purge_all", desc: { en: "Delete ALL stored messages for all users", fa: "حذف تمام پیام‌های ذخیره‌شده تمام کاربران", ru: "Удалить ВСЕ сохранённые сообщения всех пользователей" } },
                { action: "purge_read", desc: { en: "Delete only read (Seen) messages for all users", fa: "حذف فقط پیام‌های خوانده‌شده (Seen) تمام کاربران", ru: "Удалить только прочитанные (Seen) сообщения всех пользователей" } }
            ],
            examples: [
                { label: { en: "Examples", fa: "نمونه‌ها", ru: "Примеры" }, code: '// Purge messages for one user\n{"method": "POST", "resource": "/admin/queue",\n "body": {"action": "purge_user", "username": "alice@example.com"}, ...}\n\n// Purge read messages for all users\n{"method": "POST", "resource": "/admin/queue",\n "body": {"action": "purge_read"}, ...}\n\n// Purge ALL messages\n{"method": "POST", "resource": "/admin/queue",\n "body": {"action": "purge_all"}, ...}' }
            ],
            notes: [
                { type: "danger", title: { en: "⚠️ Warning", fa: "⚠️ هشدار", ru: "⚠️ Предупреждение" }, text: { en: "<code>purge_all</code> deletes all emails for all users on the server. User accounts are preserved.", fa: "عملیات <code>purge_all</code> تمام ایمیل‌های تمام کاربران در سرور را حذف می‌کند. حساب‌های کاربری حفظ می‌شوند.", ru: "Операция <code>purge_all</code> удаляет все письма всех пользователей на сервере. Учётные записи сохраняются." } }
            ]
        },
        {
            id: "shares", resource: "/admin/shares", methods: ["GET", "POST", "PUT", "DELETE"],
            title: { en: "Contact Sharing", fa: "اشتراک‌گذاری مخاطبین", ru: "Общий доступ к контактам" },
            desc: { en: "Manage contact sharing links. Only available when sharing is enabled.", fa: "مدیریت لینک‌های اشتراک‌گذاری مخاطبین. فقط در صورت فعال بودن اشتراک‌گذاری در تنظیمات.", ru: "Управление ссылками для обмена контактами. Доступно только при включённой функции обмена." },
            examples: [
                { label: { en: "List", fa: "لیست", ru: "Список" }, code: '// GET Response\n{\n    "body": {\n        "total": 2,\n        "shares": [\n            {"slug": "support", "url": "openpgp4fpr:ABCD...", "name": "Support Team"},\n            {"slug": "admin", "url": "openpgp4fpr:EFGH...", "name": "Admin"}\n        ]\n    }\n}' },
                { label: { en: "Create", fa: "ایجاد", ru: "Создать" }, code: '{"method": "POST", "resource": "/admin/shares",\n "body": {\n    "slug": "support",\n    "url": "openpgp4fpr:ABCDEF123456...",\n    "name": "Support Team"\n }, ...}' },
                { label: { en: "Update", fa: "ویرایش", ru: "Обновить" }, code: '{"method": "PUT", "resource": "/admin/shares",\n "body": {\n    "slug": "support",\n    "name": "New Name"\n }, ...}' },
                { label: { en: "Delete", fa: "حذف", ru: "Удалить" }, code: '{"method": "DELETE", "resource": "/admin/shares",\n "body": {"slug": "support"}, ...}' }
            ]
        },
        {
            id: "dns", resource: "/admin/dns", methods: ["GET", "POST", "DELETE"],
            title: { en: "DNS Rewrite", fa: "بازنویسی DNS", ru: "Перезапись DNS" },
            desc: { en: "Manage internal DNS rewrite rules. These rules change the routing of outgoing emails.", fa: "مدیریت قوانین بازنویسی DNS داخلی. این قوانین مسیر ارسال ایمیل‌ها را تغییر می‌دهند.", ru: "Управление правилами перезаписи DNS. Эти правила меняют маршрутизацию исходящих писем." },
            examples: [
                { label: { en: "List all rules", fa: "لیست تمام قوانین", ru: "Список всех правил" }, code: '// GET Response\n{\n    "body": {\n        "total": 1,\n        "overrides": [\n            {\n                "lookup_key": "old-server.example.com",\n                "target_host": "new-server.example.com",\n                "comment": "migration in progress"\n            }\n        ]\n    }\n}' },
                { label: { en: "Create/Update", fa: "ایجاد یا ویرایش", ru: "Создать/Обновить" }, code: '{"method": "POST", "resource": "/admin/dns",\n "body": {\n    "lookup_key": "old.example.com",\n    "target_host": "10.0.0.5",\n    "comment": "Route to internal server"\n }, ...}' },
                { label: { en: "Delete", fa: "حذف", ru: "Удалить" }, code: '{"method": "DELETE", "resource": "/admin/dns",\n "body": {"lookup_key": "old.example.com"}, ...}' }
            ],
            notes: [
                { type: "info", title: { en: "💡 Note", fa: "💡 نکته", ru: "💡 Примечание" }, text: { en: "DNS changes take effect immediately and do not require a server restart.", fa: "تغییرات DNS بلافاصله اعمال می‌شوند و نیازی به ریستارت سرور نیست.", ru: "Изменения DNS вступают в силу немедленно и не требуют перезапуска сервера." } }
            ]
        }
    ],

    // ── Web Admin Panel ──
    webPanel: {
        id: "web-panel",
        title: { en: "Web Admin Panel", fa: "رابط وب مدیریت", ru: "Веб-панель администратора" },
        desc: { en: "All Admin API operations are also accessible via a built-in web interface at <code>/admin/</code>. This panel uses the same authentication token.", fa: "تمام عملیات‌های Admin API از طریق یک رابط وب داخلی در مسیر <code>/admin/</code> نیز قابل دسترسی هستند. این پنل از همان توکن احراز هویت استفاده می‌کند.", ru: "Все операции Admin API также доступны через встроенный веб-интерфейс по адресу <code>/admin/</code>. Панель использует тот же токен аутентификации." },
        pages: [
            { name: "Overview", desc: { en: "Server stats (users, uptime, disk, storage), active connections (IMAP, TURN, Shadowsocks), disk usage bar, queue purge buttons", fa: "آمار سرور (کاربران، آپتایم، دیسک، ذخیره‌سازی)، اتصالات فعال (IMAP، TURN، Shadowsocks)، نوار مصرف دیسک، دکمه‌های پاکسازی صف", ru: "Статистика сервера (пользователи, аптайм, диск, хранилище), активные подключения (IMAP, TURN, Shadowsocks), индикатор использования диска, кнопки очистки очереди" } },
            { name: "Services", desc: { en: "Toggle switches for registration, JIT, TURN, Iroh, Shadowsocks and logging", fa: "کلیدهای فعال/غیرفعال برای ثبت‌نام، JIT، TURN، Iroh، Shadowsocks و لاگ", ru: "Переключатели для регистрации, JIT, TURN, Iroh, Shadowsocks и логирования" } },
            { name: "Ports", desc: { en: "View and change port numbers and configuration settings", fa: "مشاهده و تغییر شماره پورت‌ها و تنظیمات پیکربندی", ru: "Просмотр и изменение номеров портов и параметров конфигурации" } },
            { name: "Accounts", desc: { en: "Account list with storage usage, delete accounts (with confirmation dialog)", fa: "لیست حساب‌ها با مصرف فضا، حذف حساب (با پنجره تأیید)", ru: "Список аккаунтов с использованием хранилища, удаление аккаунтов (с диалогом подтверждения)" } },
            { name: "Blocked", desc: { en: "View blocked users, unblock (with confirmation dialog)", fa: "مشاهده کاربران مسدود شده، رفع مسدودیت (با پنجره تأیید)", ru: "Просмотр заблокированных пользователей, разблокировка (с диалогом подтверждения)" } },
            { name: "DNS", desc: { en: "View, add, search and delete DNS overrides (with confirmation dialog)", fa: "مشاهده، افزودن، جستجو و حذف بازنویسی‌های DNS (با پنجره تأیید)", ru: "Просмотр, добавление, поиск и удаление DNS-перезаписей (с диалогом подтверждения)" } }
        ],
        note: { en: "The admin panel supports <strong>light and dark</strong> themes (toggle in header) and is available in Farsi, English, Spanish, and Russian.", fa: "پنل مدیریت از حالت <strong>روشن و تاریک</strong> پشتیبانی می‌کند (با دکمه تغییر تم در هدر) و به زبان‌های فارسی، انگلیسی، اسپانیایی و روسی موجود است.", ru: "Панель администратора поддерживает <strong>светлую и тёмную</strong> темы (переключатель в заголовке) и доступна на фарси, английском, испанском и русском языках." }
    },

    // ── Error Codes ──
    errorCodes: {
        id: "errors",
        title: { en: "Error Codes", fa: "کدهای خطا", ru: "Коды ошибок" },
        desc: { en: "Internal status codes (in the JSON response <code>status</code> field):", fa: "کدهای وضعیت داخلی (در فیلد <code>status</code> پاسخ JSON):", ru: "Внутренние коды статуса (в поле <code>status</code> JSON-ответа):" },
        codes: [
            { code: "200", meaning: "Success", when: "Operation completed successfully" },
            { code: "201", meaning: "Created", when: "New resource created (DNS override, share)" },
            { code: "400", meaning: "Bad Request", when: "Missing required fields or invalid action" },
            { code: "401", meaning: "Unauthorized", when: "Missing, wrong, or rate-limited token" },
            { code: "404", meaning: "Not Found", when: "Unknown resource path or entry not found" },
            { code: "405", meaning: "Method Not Allowed", when: "Using wrong method for this resource" },
            { code: "500", meaning: "Internal Error", when: "Server-side failure (database, filesystem)" },
            { code: "503", meaning: "Unavailable", when: "Feature not enabled (e.g. sharing, DNS)" }
        ]
    }
};
