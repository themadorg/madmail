// Minimal i18n system — no external libraries needed.
// Supports: en, fa, es, ru

export type Locale = 'en' | 'fa' | 'es' | 'ru';

export const LOCALES: { code: Locale; label: string; flag: string }[] = [
    { code: 'en', label: 'English', flag: '🇬🇧' },
    { code: 'fa', label: 'فارسی', flag: '🇮🇷' },
    { code: 'es', label: 'Español', flag: '🇪🇸' },
    { code: 'ru', label: 'Русский', flag: '🇷🇺' },
];

export const RTL_LOCALES: Locale[] = ['fa'];

const translations: Record<Locale, Record<string, string>> = {
    en: {
        // Login
        'login.title': 'Madmail Admin',
        'login.subtitle': 'Connect to your server',
        'login.url_label': 'Admin API URL',
        'login.url_placeholder': 'https://your-server.com:443/api/admin',
        'login.token_label': 'Admin Token',
        'login.token_placeholder': 'Paste token here',
        'login.connect': 'Connect',
        'login.connecting': 'Connecting…',

        // Tabs
        'tab.overview': 'Overview',
        'tab.services': 'Services',
        'tab.ports': 'Ports',
        'tab.accounts': 'Accounts',
        'tab.blocked': 'Blocked',
        'tab.dns': 'DNS',

        // Overview cards
        'stat.users': 'Users',
        'stat.uptime': 'Uptime',
        'stat.disk': 'Disk',
        'stat.storage': 'Storage',
        'stat.sent': 'Messages Sent',
        'stat.outbound': 'Messages Out',
        'stat.received': 'Messages In',
        'stat.imap': 'IMAP',
        'stat.turn_relays': 'TURN Relays',
        'stat.ss_conns': 'SS Conns',

        // Disk
        'disk.title': 'Disk Usage',
        'disk.used': 'used',
        'disk.free_of': 'free of',

        // Email Traffic
        'traffic.title': 'Email Traffic',
        'traffic.connections': 'Connections',
        'traffic.domains': 'Domains',
        'traffic.ip_servers': 'IP Servers',

        // Queue
        'queue.title': 'Queue',
        'queue.purge_read': 'Purge Read',
        'queue.purge_all': 'Purge All',

        // Services tab
        'svc.registration': 'Registration',
        'svc.jit_registration': 'JIT Registration',
        'svc.turn': 'TURN',
        'svc.iroh': 'Iroh Relay',
        'svc.shadowsocks': 'Shadowsocks',
        'svc.configuration': 'Configuration',
        'svc.smtp_hostname': 'SMTP Hostname',
        'svc.turn_realm': 'TURN Realm',
        'svc.turn_secret': 'TURN Secret',
        'svc.turn_relay_ip': 'TURN Relay IP',
        'svc.turn_ttl': 'TURN TTL',
        'svc.iroh_relay_url': 'Iroh Relay URL',
        'svc.ss_cipher': 'SS Cipher',
        'svc.ss_password': 'SS Password',
        'svc.admin_path': 'Admin API Path',

        // Ports tab
        'port.smtp': 'SMTP',
        'port.submission': 'Submission',
        'port.imap': 'IMAP',
        'port.turn': 'TURN',
        'port.sasl': 'SASL',
        'port.iroh': 'Iroh',
        'port.shadowsocks': 'Shadowsocks',
        'port.http': 'HTTP',
        'port.https': 'HTTPS',
        'port.public': 'Public',
        'port.local': 'Local Only',
        'port.access_hint': 'Local = accessible only via Shadowsocks',
        'port.confirm_local': 'Make {port} local only?',
        'port.confirm_local_warn': 'This port will only be accessible via Shadowsocks. Direct connections from the internet will be blocked. This requires a restart.',
        'port.confirm_yes': 'Yes, make local',
        'port.confirm_no': 'Cancel',

        // Accounts tab
        'acct.total': 'accounts',
        'acct.used': 'used',
        'acct.quota': 'quota',
        'acct.confirm_delete': 'Delete {username} and block? This is irreversible.',
        'acct.confirm_yes': 'Yes, Delete',
        'acct.confirm_no': 'Cancel',
        'acct.sort_name': 'Name',
        'acct.sort_size': 'Size',
        'acct.sort_date': 'Created',
        'acct.sort_login': 'Last Login',
        'acct.created': 'Created',
        'acct.last_login': 'Last Login',
        'acct.never': 'Never',
        'acct.blocked': 'Blocked',
        'acct.unblock': 'Unblock',
        'acct.blocked_count': '{count} blocked',
        'acct.create': 'Create Account',
        'acct.create_hint': 'Registration is closed. Create accounts manually for users.',
        'acct.new_account': 'New Account Created',
        'acct.dclogin_warning': 'Save this link now! It will only be shown once.',
        'acct.copy_dclogin': 'Copy dclogin Link',
        'acct.open_dc': 'Open in Delta Chat',
        'acct.dismiss': 'Done',
        'acct.default_quota': 'default quota',
        'acct.reset_quota': 'Reset to default',

        // DNS tab
        'dns.total': '{count} overrides',
        'dns.lookup': 'Lookup Key',
        'dns.target': 'Target Host',
        'dns.comment': 'Comment',
        'dns.add': 'Add Override',
        'dns.empty': 'No DNS overrides',
        'dns.confirm_delete': 'Remove override for {key}?',

        // Actions
        'action.save': 'Save',
        'action.cancel': 'Cancel',
        'action.apply_restart': 'Apply & Restart',
        'action.restarting': 'Restarting…',
        'action.restart_needed': 'Changes need a restart',
        'action.refresh': 'Refresh',
        'action.disconnect': 'Disconnect',

        // Notifications
        'notify.online': 'Online',
        'notify.restart_pending': 'May still be restarting',
        'notify.updated': '{key} updated',
        'notify.reset': '{key} reset',
        'notify.deleted': 'Deleted {username}',
        'notify.purge_done': '{action} done',
        'notify.unblocked': '{username} unblocked',
        'notify.account_created': 'Account created: {email}',
        'notify.dns_added': 'DNS override added: {key}',
        'notify.dns_deleted': 'DNS override removed: {key}',
        'notify.restarting': 'Restarting service...',
        'notify.quota_updated': 'Quota updated',
        'notify.quota_reset': 'Quota reset to default',

        // Misc
        'misc.default': '(default)',
        'misc.loading': 'Loading…',
        'misc.language': 'Language',
    },

    fa: {
        'login.title': 'مدیریت مدمیل',
        'login.subtitle': 'اتصال به سرور شما',
        'login.url_label': 'آدرس API مدیریت',
        'login.url_placeholder': 'https://your-server.com:443/api/admin',
        'login.token_label': 'توکن مدیریت',
        'login.token_placeholder': 'توکن را اینجا وارد کنید',
        'login.connect': 'اتصال',
        'login.connecting': 'در حال اتصال…',

        'tab.overview': 'نمای کلی',
        'tab.services': 'سرویس‌ها',
        'tab.ports': 'پورت‌ها',
        'tab.accounts': 'حساب‌ها',
        'tab.blocked': 'مسدود',
        'tab.dns': 'DNS',

        'stat.users': 'کاربران',
        'stat.uptime': 'آپتایم',
        'stat.disk': 'دیسک',
        'stat.storage': 'فضا',
        'stat.sent': 'پیام ارسالی',
        'stat.outbound': 'پیام خروجی',
        'stat.received': 'پیام ورودی',
        'stat.imap': 'IMAP',
        'stat.turn_relays': 'رله TURN',
        'stat.ss_conns': 'اتصالات SS',

        'disk.title': 'فضای دیسک',
        'disk.used': 'استفاده‌شده',
        'disk.free_of': 'آزاد از',

        'traffic.title': 'ترافیک ایمیل',
        'traffic.connections': 'اتصالات',
        'traffic.domains': 'دامنه‌ها',
        'traffic.ip_servers': 'سرورهای IP',

        'queue.title': 'صف',
        'queue.purge_read': 'پاکسازی خوانده‌شده',
        'queue.purge_all': 'پاکسازی همه',

        'svc.registration': 'ثبت‌نام',
        'svc.jit_registration': 'ثبت‌نام آنی',
        'svc.turn': 'TURN',
        'svc.iroh': 'رله Iroh',
        'svc.shadowsocks': 'شدوساکس',
        'svc.configuration': 'تنظیمات',
        'svc.smtp_hostname': 'نام هاست SMTP',
        'svc.turn_realm': 'قلمرو TURN',
        'svc.turn_secret': 'رمز TURN',
        'svc.turn_relay_ip': 'IP رله TURN',
        'svc.turn_ttl': 'TTL TURN',
        'svc.iroh_relay_url': 'آدرس رله Iroh',
        'svc.ss_cipher': 'رمزنگاری SS',
        'svc.ss_password': 'رمز عبور SS',
        'svc.admin_path': 'مسیر API مدیریت',

        'port.smtp': 'SMTP',
        'port.submission': 'ارسال',
        'port.imap': 'IMAP',
        'port.turn': 'TURN',
        'port.sasl': 'SASL',
        'port.iroh': 'Iroh',
        'port.shadowsocks': 'شدوساکس',
        'port.http': 'HTTP',
        'port.https': 'HTTPS',
        'port.public': 'عمومی',
        'port.local': 'فقط محلی',
        'port.access_hint': 'محلی = فقط از طریق شدوساکس قابل دسترسی',
        'port.confirm_local': 'پورت {port} را فقط محلی کنید؟',
        'port.confirm_local_warn': 'این پورت فقط از طریق شدوساکس قابل دسترسی خواهد بود. اتصالات مستقیم از اینترنت مسدود می‌شوند. نیاز به راه‌اندازی مجدد دارد.',
        'port.confirm_yes': 'بله، محلی کن',
        'port.confirm_no': 'لغو',

        'acct.total': 'حساب',
        'acct.used': 'استفاده‌شده',
        'acct.quota': 'سهمیه',
        'acct.confirm_delete': '{username} حذف و مسدود شود؟ این عمل غیرقابل بازگشت است.',
        'acct.confirm_yes': 'بله، حذف کن',
        'acct.confirm_no': 'انصراف',
        'acct.sort_name': 'نام',
        'acct.sort_size': 'حجم',
        'acct.sort_date': 'تاریخ ایجاد',
        'acct.sort_login': 'آخرین ورود',
        'acct.created': 'ایجاد',
        'acct.last_login': 'آخرین ورود',
        'acct.never': 'هرگز',
        'acct.blocked': 'مسدود',
        'acct.unblock': 'رفع مسدودی',
        'acct.blocked_count': '{count} مسدود',
        'acct.create': 'ایجاد حساب',
        'acct.create_hint': 'ثبت‌نام بسته است. حساب‌ها را به صورت دستی ایجاد کنید.',
        'acct.new_account': 'حساب جدید ایجاد شد',
        'acct.dclogin_warning': 'این لینک را ذخیره کنید! فقط یک بار نمایش داده می‌شود.',
        'acct.copy_dclogin': 'کپی لینک dclogin',
        'acct.open_dc': 'باز کردن در دلتاچت',
        'acct.dismiss': 'انجام',
        'acct.default_quota': 'سهمیه پیش‌فرض',
        'acct.reset_quota': 'بازگشت به پیش‌فرض',

        'dns.total': '{count} بازنویسی',
        'dns.lookup': 'کلید جستجو',
        'dns.target': 'میزبان مقصد',
        'dns.comment': 'توضیحات',
        'dns.add': 'افزودن بازنویسی',
        'dns.empty': 'بازنویسی DNS وجود ندارد',
        'dns.confirm_delete': 'بازنویسی {key} حذف شود؟',

        'action.save': 'ذخیره',
        'action.cancel': 'لغو',
        'action.apply_restart': 'اعمال و ری‌استارت',
        'action.restarting': 'در حال ری‌استارت…',
        'action.restart_needed': 'تغییرات نیاز به ری‌استارت دارند',
        'action.refresh': 'بازنشانی',
        'action.disconnect': 'قطع اتصال',

        'notify.online': 'آنلاین',
        'notify.restart_pending': 'ممکن است هنوز در حال ری‌استارت باشد',
        'notify.updated': '{key} بروز شد',
        'notify.reset': '{key} بازنشانی شد',
        'notify.deleted': '{username} حذف شد',
        'notify.purge_done': '{action} انجام شد',
        'notify.unblocked': '{username} رفع مسدودی شد',
        'notify.account_created': 'حساب ایجاد شد: {email}',
        'notify.dns_added': 'بازنویسی DNS اضافه شد: {key}',
        'notify.dns_deleted': 'بازنویسی DNS حذف شد: {key}',
        'notify.restarting': 'در حال ری‌استارت سرویس...',
        'notify.quota_updated': 'سهمیه به‌روز شد',
        'notify.quota_reset': 'سهمیه به پیش‌فرض بازگشت',

        'misc.default': '(پیش‌فرض)',
        'misc.loading': 'بارگذاری…',
        'misc.language': 'زبان',
    },

    es: {
        'login.title': 'Admin Madmail',
        'login.subtitle': 'Conectar a tu servidor',
        'login.url_label': 'URL de API Admin',
        'login.url_placeholder': 'https://tu-servidor.com:443/api/admin',
        'login.token_label': 'Token de Admin',
        'login.token_placeholder': 'Pegar token aquí',
        'login.connect': 'Conectar',
        'login.connecting': 'Conectando…',

        'tab.overview': 'General',
        'tab.services': 'Servicios',
        'tab.ports': 'Puertos',
        'tab.accounts': 'Cuentas',
        'tab.blocked': 'Bloqueados',
        'tab.dns': 'DNS',

        'stat.users': 'Usuarios',
        'stat.uptime': 'Tiempo activo',
        'stat.disk': 'Disco',
        'stat.storage': 'Almacenamiento',
        'stat.sent': 'Mensajes Enviados',
        'stat.outbound': 'Mensajes Salientes',
        'stat.received': 'Mensajes Recibidos',
        'stat.imap': 'IMAP',
        'stat.turn_relays': 'Relés TURN',
        'stat.ss_conns': 'Conex. SS',

        'disk.title': 'Uso de disco',
        'disk.used': 'usado',
        'disk.free_of': 'libre de',

        'traffic.title': 'Tráfico de correo',
        'traffic.connections': 'Conexiones',
        'traffic.domains': 'Dominios',
        'traffic.ip_servers': 'Servidores IP',

        'queue.title': 'Cola',
        'queue.purge_read': 'Purgar leídos',
        'queue.purge_all': 'Purgar todo',

        'svc.registration': 'Registro',
        'svc.jit_registration': 'Registro JIT',
        'svc.turn': 'TURN',
        'svc.iroh': 'Relé Iroh',
        'svc.shadowsocks': 'Shadowsocks',
        'svc.configuration': 'Configuración',
        'svc.smtp_hostname': 'Nombre de host SMTP',
        'svc.turn_realm': 'Dominio TURN',
        'svc.turn_secret': 'Secreto TURN',
        'svc.turn_relay_ip': 'IP de relé TURN',
        'svc.turn_ttl': 'TTL TURN',
        'svc.iroh_relay_url': 'URL de relé Iroh',
        'svc.ss_cipher': 'Cifrado SS',
        'svc.ss_password': 'Contraseña SS',
        'svc.admin_path': 'Ruta API Admin',

        'port.smtp': 'SMTP',
        'port.submission': 'Envío',
        'port.imap': 'IMAP',
        'port.turn': 'TURN',
        'port.sasl': 'SASL',
        'port.iroh': 'Iroh',
        'port.shadowsocks': 'Shadowsocks',
        'port.http': 'HTTP',
        'port.https': 'HTTPS',
        'port.public': 'Público',
        'port.local': 'Solo Local',
        'port.access_hint': 'Local = accesible solo a través de Shadowsocks',
        'port.confirm_local': '¿Hacer {port} solo local?',
        'port.confirm_local_warn': 'Este puerto solo será accesible a través de Shadowsocks. Las conexiones directas desde internet serán bloqueadas. Requiere reinicio.',
        'port.confirm_yes': 'Sí, hacer local',
        'port.confirm_no': 'Cancelar',

        'acct.total': 'cuentas',
        'acct.used': 'usado',
        'acct.quota': 'cuota',
        'acct.confirm_delete': '¿Eliminar {username} y bloquear? Esto es irreversible.',
        'acct.confirm_yes': 'Sí, eliminar',
        'acct.confirm_no': 'Cancelar',
        'acct.sort_name': 'Nombre',
        'acct.sort_size': 'Tamaño',
        'acct.sort_date': 'Creado',
        'acct.sort_login': 'Último Acceso',
        'acct.created': 'Creado',
        'acct.last_login': 'Último Acceso',
        'acct.never': 'Nunca',
        'acct.blocked': 'Bloqueado',
        'acct.unblock': 'Desbloquear',
        'acct.blocked_count': '{count} bloqueados',
        'acct.create': 'Crear Cuenta',
        'acct.create_hint': 'El registro está cerrado. Cree cuentas manualmente.',
        'acct.new_account': 'Nueva Cuenta Creada',
        'acct.dclogin_warning': '¡Guarde este enlace ahora! Solo se mostrará una vez.',
        'acct.copy_dclogin': 'Copiar enlace dclogin',
        'acct.open_dc': 'Abrir en Delta Chat',
        'acct.dismiss': 'Listo',
        'acct.default_quota': 'cuota por defecto',
        'acct.reset_quota': 'Restablecer a defecto',

        'dns.total': '{count} sobrecargas',
        'dns.lookup': 'Clave de búsqueda',
        'dns.target': 'Host destino',
        'dns.comment': 'Comentario',
        'dns.add': 'Agregar',
        'dns.empty': 'Sin sobrecargas DNS',
        'dns.confirm_delete': '¿Eliminar sobrecarga para {key}?',

        'action.save': 'Guardar',
        'action.cancel': 'Cancelar',
        'action.apply_restart': 'Aplicar y reiniciar',
        'action.restarting': 'Reiniciando…',
        'action.restart_needed': 'Los cambios requieren reinicio',
        'action.refresh': 'Actualizar',
        'action.disconnect': 'Desconectar',

        'notify.online': 'En línea',
        'notify.restart_pending': 'Puede estar reiniciando',
        'notify.updated': '{key} actualizado',
        'notify.reset': '{key} restablecido',
        'notify.deleted': '{username} eliminado',
        'notify.purge_done': '{action} completado',
        'notify.unblocked': '{username} desbloqueado',
        'notify.account_created': 'Cuenta creada: {email}',
        'notify.dns_added': 'Sobrecarga DNS agregada: {key}',
        'notify.dns_deleted': 'Sobrecarga DNS eliminada: {key}',
        'notify.restarting': 'Reiniciando servicio...',
        'notify.quota_updated': 'Cuota actualizada',
        'notify.quota_reset': 'Cuota restablecida',

        'misc.default': '(predeterminado)',
        'misc.loading': 'Cargando…',
        'misc.language': 'Idioma',
    },

    ru: {
        'login.title': 'Админ Madmail',
        'login.subtitle': 'Подключение к серверу',
        'login.url_label': 'URL API администратора',
        'login.url_placeholder': 'https://ваш-сервер.com:443/api/admin',
        'login.token_label': 'Токен администратора',
        'login.token_placeholder': 'Вставьте токен сюда',
        'login.connect': 'Подключить',
        'login.connecting': 'Подключение…',

        'tab.overview': 'Обзор',
        'tab.services': 'Сервисы',
        'tab.ports': 'Порты',
        'tab.accounts': 'Аккаунты',
        'tab.blocked': 'Заблокированные',
        'tab.dns': 'DNS',

        'stat.users': 'Пользователи',
        'stat.uptime': 'Аптайм',
        'stat.disk': 'Диск',
        'stat.storage': 'Хранилище',
        'stat.sent': 'Отправлено',
        'stat.outbound': 'Исходящие',
        'stat.received': 'Входящие',
        'stat.imap': 'IMAP',
        'stat.turn_relays': 'TURN релеи',
        'stat.ss_conns': 'SS соединения',

        'disk.title': 'Использование диска',
        'disk.used': 'использовано',
        'disk.free_of': 'свободно из',

        'traffic.title': 'Почтовый трафик',
        'traffic.connections': 'Соединения',
        'traffic.domains': 'Домены',
        'traffic.ip_servers': 'IP серверы',

        'queue.title': 'Очередь',
        'queue.purge_read': 'Очистить прочитанные',
        'queue.purge_all': 'Очистить всё',

        'svc.registration': 'Регистрация',
        'svc.jit_registration': 'JIT Регистрация',
        'svc.turn': 'TURN',
        'svc.iroh': 'Реле Iroh',
        'svc.shadowsocks': 'Shadowsocks',
        'svc.configuration': 'Конфигурация',
        'svc.smtp_hostname': 'Имя хоста SMTP',
        'svc.turn_realm': 'Область TURN',
        'svc.turn_secret': 'Секрет TURN',
        'svc.turn_relay_ip': 'IP реле TURN',
        'svc.turn_ttl': 'TTL TURN',
        'svc.iroh_relay_url': 'URL реле Iroh',
        'svc.ss_cipher': 'Шифр SS',
        'svc.ss_password': 'Пароль SS',
        'svc.admin_path': 'Путь API администратора',

        'port.smtp': 'SMTP',
        'port.submission': 'Отправка',
        'port.imap': 'IMAP',
        'port.turn': 'TURN',
        'port.sasl': 'SASL',
        'port.iroh': 'Iroh',
        'port.shadowsocks': 'Shadowsocks',
        'port.http': 'HTTP',
        'port.https': 'HTTPS',
        'port.public': 'Публичный',
        'port.local': 'Только локальный',
        'port.access_hint': 'Локальный = доступен только через Shadowsocks',
        'port.confirm_local': 'Сделать {port} только локальным?',
        'port.confirm_local_warn': 'Этот порт будет доступен только через Shadowsocks. Прямые подключения из интернета будут заблокированы. Требуется перезагрузка.',
        'port.confirm_yes': 'Да, сделать локальным',
        'port.confirm_no': 'Отмена',

        'acct.total': 'аккаунтов',
        'acct.used': 'использовано',
        'acct.quota': 'квота',
        'acct.confirm_delete': 'Удалить {username} и заблокировать? Это необратимо.',
        'acct.confirm_yes': 'Да, удалить',
        'acct.confirm_no': 'Отмена',
        'acct.sort_name': 'Имя',
        'acct.sort_size': 'Размер',
        'acct.sort_date': 'Создан',
        'acct.sort_login': 'Посл. вход',
        'acct.created': 'Создан',
        'acct.last_login': 'Посл. вход',
        'acct.never': 'Никогда',
        'acct.blocked': 'Заблокирован',
        'acct.unblock': 'Разблокировать',
        'acct.blocked_count': '{count} заблокировано',
        'acct.create': 'Создать Аккаунт',
        'acct.create_hint': 'Регистрация закрыта. Создайте аккаунты вручную.',
        'acct.new_account': 'Новый Аккаунт Создан',
        'acct.dclogin_warning': 'Сохраните эту ссылку! Она будет показана только один раз.',
        'acct.copy_dclogin': 'Копировать ссылку dclogin',
        'acct.open_dc': 'Открыть в Delta Chat',
        'acct.dismiss': 'Готово',
        'acct.default_quota': 'квота по умолчанию',
        'acct.reset_quota': 'Сбросить',

        'dns.total': '{count} переопределений',
        'dns.lookup': 'Ключ поиска',
        'dns.target': 'Целевой хост',
        'dns.comment': 'Комментарий',
        'dns.add': 'Добавить',
        'dns.empty': 'Нет переопределений DNS',
        'dns.confirm_delete': 'Удалить переопределение для {key}?',

        'action.save': 'Сохранить',
        'action.cancel': 'Отмена',
        'action.apply_restart': 'Применить и перезапустить',
        'action.restarting': 'Перезапуск…',
        'action.restart_needed': 'Изменения требуют перезапуска',
        'action.refresh': 'Обновить',
        'action.disconnect': 'Отключить',

        'notify.online': 'Онлайн',
        'notify.restart_pending': 'Возможно, ещё перезапускается',
        'notify.updated': '{key} обновлено',
        'notify.reset': '{key} сброшено',
        'notify.deleted': '{username} удалён',
        'notify.purge_done': '{action} выполнено',
        'notify.unblocked': '{username} разблокирован',
        'notify.account_created': 'Аккаунт создан: {email}',
        'notify.dns_added': 'Переопределение DNS добавлено: {key}',
        'notify.dns_deleted': 'Переопределение DNS удалено: {key}',
        'notify.restarting': 'Перезапуск сервиса...',
        'notify.quota_updated': 'Квота обновлена',
        'notify.quota_reset': 'Квота сброшена',

        'misc.default': '(по умолчанию)',
        'misc.loading': 'Загрузка…',
        'misc.language': 'Язык',
    },
};

// --- Reactive locale state ---
let _locale: Locale = (localStorage.getItem('madmail_locale') as Locale) || 'en';

export function getLocale(): Locale {
    return _locale;
}

function applyLocaleStyles(l: Locale) {
    document.documentElement.dir = RTL_LOCALES.includes(l) ? 'rtl' : 'ltr';
    document.documentElement.lang = l;
    // Set font family based on locale
    const fontFamily = RTL_LOCALES.includes(l)
        ? "'Shabnam', 'Inter', system-ui, sans-serif"
        : "'Inter', system-ui, sans-serif";
    document.body.style.fontFamily = fontFamily;
}

export function setLocale(l: Locale) {
    _locale = l;
    localStorage.setItem('madmail_locale', l);
    applyLocaleStyles(l);
}

// Initialize direction + font on load
if (typeof document !== 'undefined') {
    applyLocaleStyles(_locale);
}

/**
 * Translate a key, optionally interpolating {placeholders}.
 *   t('notify.deleted', { username: 'foo@bar' })  →  'Deleted foo@bar'
 */
export function t(key: string, params?: Record<string, string>): string {
    let str = translations[_locale]?.[key] ?? translations.en[key] ?? key;
    if (params) {
        for (const [k, v] of Object.entries(params)) {
            str = str.replace(`{${k}}`, v);
        }
    }
    return str;
}

/**
 * Check if the current locale is RTL.
 */
export function isRTL(): boolean {
    return RTL_LOCALES.includes(_locale);
}
