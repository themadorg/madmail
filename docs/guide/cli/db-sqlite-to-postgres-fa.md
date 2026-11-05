# فرمان انتقال SQLite به PostgreSQL

فرمان `madmail db sqlite-to-postgres` جدول‌های برنامه را از SQLite به PostgreSQL کپی می‌کند. پیش از اجرا، **سرویس را متوقف کنید و پشتیبان بگیرید**. فایل تنظیمات خودکار بازنویسی نمی‌شود؛ پس از انتقال، `driver postgres` و DSN مقصد را تنظیم کنید و سرویس را راه‌اندازی کنید.

## شکل فرمان

```bash
madmail db sqlite-to-postgres --dsn <DSN> [--sqlite PATH] [--dry-run] [--force] [-y]
```

## گزینه‌ها

| گزینه | کاربرد |
| --- | --- |
| `--dsn` | نشانی PostgreSQL یا رشتهٔ اتصال `key=value` |
| `--sqlite` | مسیر فایل SQLite؛ پیش‌فرض از تنظیمات برنامه خوانده می‌شود |
| `--dry-run` | فقط شمارش ردیف‌های SQLite؛ به PostgreSQL وصل نمی‌شود |
| `--force` | پاک‌کردن داده‌های قبلیِ جدول‌های قابل انتقال در مقصد، پیش از کپی |
| `-y` یا `--yes` | اجرای انتقال بدون پرسش تأیید |

## نمونه

در این نمونه، Madmail روی میزبان و PostgreSQL روی localhost است. رمز و مسیرها را مطابق سرور خود تغییر دهید:

```bash
sudo madmail --config /etc/madmail/madmail.conf db sqlite-to-postgres \
    --sqlite /var/lib/madmail/credentials.db \
    --dsn 'host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable' --dry-run

sudo madmail --config /etc/madmail/madmail.conf db sqlite-to-postgres \
    --sqlite /var/lib/madmail/credentials.db \
    --dsn 'host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable' -y
```

برای این مسیر از پایگاه مقصد تازه و خالی استفاده کنید. `--force` برای ادامهٔ عادی انتقال لازم نیست و داده‌های مقصد را حذف می‌کند.

## نکته‌ها

- انتقال واقعی ساختار v2 را در PostgreSQL آماده می‌کند. کپی و حذف‌های `--force` در یک تراکنش انجام می‌شوند؛ شکست کپی، تغییرات آن تراکنش را برمی‌گرداند. ساختار آماده‌شدهٔ پایگاه ممکن است باقی بماند.
- مقصد با جدول قدیمی `passwords` به شکل کلید/مقدار پذیرفته نمی‌شود؛ ابتدا باید ساختار آن به v2 ارتقا پیدا کند.
- جدول‌ها و ستون‌های SQLite نسخهٔ قدیمی Go شناسایی می‌شوند؛ ستون `source_table` در گزارش نام مبدأ را نشان می‌دهد.
- Maildir، صف ارسال، `admin_token` و `sharing.db` کپی نمی‌شوند.

## خروجی JSON

```bash
madmail db sqlite-to-postgres --dsn 'postgres://madmail@127.0.0.1/madmail' --dry-run --json
```

این نمونه فقط شمارش است و به رمز یا اتصال PostgreSQL نیاز ندارد.

[راهنمای نصب و انتقال](../../project/user-guide/18-postgres-fa.md) · [مرجع انگلیسی](db-sqlite-to-postgres.md)
