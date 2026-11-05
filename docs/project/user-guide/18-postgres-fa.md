# نصب PostgreSQL و انتقال از SQLite

Madmail v2 از **SQLite** (پیش‌فرض) و **PostgreSQL** پشتیبانی می‌کند؛ **MySQL فعلا پشتیبانی نمی‌شود**. در این راهنما Madmail روی میزبان نصب است و PostgreSQL روی همان سرور، با Docker یا مدیر بسته نصب می‌شود.

حساب‌ها، تنظیمات، سهمیه‌ها و وضعیت فدراسیون در یک پایگاه SQL ذخیره می‌شوند. نامه‌ها در Maildir، صف ارسال در `remote_queue/`، توکن مدیریت در `admin_token` و اطلاعات اشتراک مخاطب در `sharing.db` می‌مانند. این فایل‌ها با ابزار انتقال SQL جابه‌جا نمی‌شوند.

## ۱. نصب PostgreSQL

فقط یکی از دو روش زیر را انتخاب کنید. رمزهای نمونه را با رمز تصادفی بلند از حروف و عدد عوض کنید تا نیاز به تغییر نقل‌قول‌های دستورها نباشد. رمز کاربر `madmail` در همهٔ مرحله‌ها باید یکسان باشد.

<div data-postgres-options>
<fieldset><legend>سیستم‌عامل میزبان</legend><div class="postgres-choices"><button type="button" data-pg-choice="os" data-pg-value="debian">Debian / Ubuntu</button><button type="button" data-pg-choice="os" data-pg-value="fedora">Fedora</button></div></fieldset>
<fieldset><legend>روش نصب PostgreSQL</legend><div class="postgres-choices"><button type="button" data-pg-choice="method" data-pg-value="docker">Docker</button><button type="button" data-pg-choice="method" data-pg-value="native">مدیر بسته</button></div></fieldset>
<p>سیستم‌عامل برای دستورهای نصب با مدیر بسته است؛ دستورهای Docker در هر دو یکسان‌اند.</p>
</div>

<div data-pg-method="docker">

### روش اول: Docker

Docker باید نصب و فعال باشد و پورت ۵۴۳۲ میزبان آزاد باشد. این دستور PostgreSQL 17 را با volume پایدار اجرا می‌کند و پورت آن فقط روی localhost در دسترس است.

```bash
sudo docker volume create madmail-postgres-data
sudo docker run -d --name madmail-postgres --restart unless-stopped \
    -p 127.0.0.1:5432:5432 \
    -e POSTGRES_PASSWORD=choose-postgres-secret \
    -v madmail-postgres-data:/var/lib/postgresql/data \
    postgres:17
```

تا نمایش `accepting connections` صبر کنید؛ در صورت نیاز دوباره اجرا کنید:

```bash
sudo docker exec madmail-postgres pg_isready -h 127.0.0.1 -U postgres
```

داده‌ها در volume می‌مانند؛ آن را حذف نکنید. برای این روش، PostgreSQL روی میزبان نصب نمی‌شود. از پایگاه داده جداگانه با ابزارهایی مانند `pg_dump` پشتیبان بگیرید.

</div>

<div data-pg-method="native">

### روش دوم: مدیر بسته

<div data-pg-os="debian">

**Debian / Ubuntu:**

```bash
sudo apt-get update
sudo apt-get install -y postgresql postgresql-client
sudo systemctl enable --now postgresql
```

</div>

<div data-pg-os="fedora">

**Fedora:** مقداردهی اولیه را فقط برای نصب تازه اجرا کنید.

```bash
sudo dnf install -y postgresql-server postgresql
sudo postgresql-setup --initdb
```

در `/var/lib/pgsql/data/pg_hba.conf`، خط موجود برای `127.0.0.1/32` را به شکل زیر تغییر دهید:

```text
host    all    all    127.0.0.1/32    scram-sha-256
```

```bash
sudo systemctl enable --now postgresql
```

</div>

</div>

راهنمای رسمی: [Docker](https://hub.docker.com/_/postgres) · [Fedora](https://developer.fedoraproject.org/tech/database/postgresql/about.html).

## ۲. ساخت کاربر و پایگاه داده

دستور مربوط به روش انتخاب‌شده را اجرا کنید. برای تغییر روش، از دکمه‌های بالا استفاده کنید.

<div data-pg-method="native">

**نصب مستقیم:**

```bash
sudo -u postgres psql -v ON_ERROR_STOP=1
```

</div>

<div data-pg-method="docker">

**Docker:**

```bash
sudo docker exec -it madmail-postgres psql -U postgres -v ON_ERROR_STOP=1
```

</div>

سپس دستورهای زیر را وارد کنید؛ `choose-a-secret` رمز کاربر Madmail است:

```sql
CREATE USER madmail WITH PASSWORD 'choose-a-secret';
CREATE DATABASE madmail OWNER madmail;
\q
```

اتصال را بررسی کنید و رمز کاربر Madmail را هنگام درخواست وارد کنید.

<div data-pg-method="native">

**نصب مستقیم:**

```bash
psql -h 127.0.0.1 -U madmail -d madmail -W -c 'SELECT current_user, current_database();'
```

</div>

<div data-pg-method="docker">

**Docker:**

```bash
sudo docker exec -it madmail-postgres psql -h 127.0.0.1 -U madmail -d madmail -W -c 'SELECT current_user, current_database();'
```

</div>

از اینجا به بعد، فرمان‌های انتقال و تنظیمات برای هر دو روش یکسان‌اند.

## ۳. توقف و پشتیبان‌گیری

Madmail باید از قبل نصب شده باشد. مسیرهای زیر پیش‌فرض‌اند؛ اگر `state_dir` یا مسیر تنظیمات فرق دارد، آن‌ها را تغییر دهید. ابتدا سرویس را متوقف کنید:

```bash
sudo systemctl stop madmail
sudo tar -czf /root/madmail-before-postgres.tar.gz /etc/madmail /var/lib/madmail
```

این فرمان فقط مسیرهای نوشته‌شده را پشتیبان می‌گیرد؛ اگر SQLite یا داده‌های شما جای دیگری است، پوشهٔ آن‌ها را هم اضافه کنید. برای هر انتقال، نام پشتیبان را عوض کنید و فایل را در جای امن نگه دارید.

## ۴. انتقال SQLite به PostgreSQL

برای نصب تازه، این مرحله را رد کنید. برای سرور موجود، مسیر SQLite را از `dsn` در `auth.pass_table` ← `table sql_table` پیدا کنید. در این نمونه `/var/lib/madmail/credentials.db` است.

```bash
sudo madmail --config /etc/madmail/madmail.conf db sqlite-to-postgres \
    --sqlite /var/lib/madmail/credentials.db \
    --dsn 'host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable' --dry-run

sudo madmail --config /etc/madmail/madmail.conf db sqlite-to-postgres \
    --sqlite /var/lib/madmail/credentials.db \
    --dsn 'host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable' -y
```

`--dry-run` فقط ردیف‌های SQLite را می‌شمارد و اتصال PostgreSQL را بررسی نمی‌کند. فرمان دوم ساختار جدول‌ها را می‌سازد و داده‌ها را کپی می‌کند؛ تعداد ردیف‌های مبدأ و کپی‌شده را در خروجی مقایسه کنید.

از یک پایگاه تازه و خالی استفاده کنید. اگر ابزار خطای پر بودن مقصد را داد، پایگاه تازه‌ای بسازید. **`--force` داده‌های قبلیِ جدول‌های قابل انتقال را از مقصد پاک می‌کند**؛ برای این راهنما از آن استفاده نکنید.

ابزار داخلی Madmail ساختار جدول‌ها را می‌شناسد؛ نیازی به `pgloader` نیست. فایل‌های نامه، صف ارسال، `admin_token` و `sharing.db` در همان پوشه می‌مانند. فایل تنظیمات خودکار تغییر نمی‌کند.

## ۵. تغییر تنظیمات

در `/etc/madmail/madmail.conf` فقط `driver` و `dsn` را در **هر دو بلوک موجود** مطابق نمونه تغییر دهید؛ بقیهٔ گزینه‌های خود را نگه دارید و بلوک تکراری نسازید.

```text
auth.pass_table local_authdb {
    auto_create yes
    jit_domain $(primary_domain)
    table sql_table {
        driver postgres
        dsn host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable
        table_name passwords
    }
}

storage.imapsql local_mailboxes {
    auto_create yes
    driver postgres
    dsn host=127.0.0.1 port=5432 user=madmail password=choose-a-secret dbname=madmail sslmode=disable
    retention 24h
    default_quota 1G
    appendlimit 100M
}
```

در v2، هر دو بخش از یک پایگاه استفاده می‌کنند؛ DSN بخش احراز هویت تعیین می‌کند کدام پایگاه باز شود. `sslmode=disable` در این نمونه فقط برای اتصال محلی است؛ برای پایگاه روی سرور دیگر TLS را تنظیم کنید. دسترسی به فایل تنظیمات و تاریخچهٔ فرمان‌ها را محدود کنید؛ رمز DSN در آن‌ها دیده می‌شود.

## ۶. راه‌اندازی و بررسی

```bash
sudo systemctl start madmail
sudo systemctl status madmail --no-pager
sudo madmail --config /etc/madmail/madmail.conf accounts status
```

تعداد حساب‌ها را بررسی کنید و با یک حساب قبلی وارد برنامهٔ ایمیل شوید. در صورت خطا:

```bash
sudo journalctl -u madmail -n 50 --no-pager
```

در نصب تازه، جدول‌ها در اولین اجرا خودکار ساخته می‌شوند. تغییر `driver` داده‌ها را منتقل نمی‌کند؛ `madmail reload` هم اتصال پایگاه داده را عوض نمی‌کند و سرویس باید دوباره راه‌اندازی شود.

اگر انتقال ناموفق بود، سرویس را متوقف نگه دارید و تنظیمات SQLite را برگردانید. تا پایان بررسی، SQLite و پشتیبان را حذف نکنید. اگر داده‌های جدیدی در PostgreSQL ثبت شود، بازگشت به SQLite آن‌ها را منتقل نمی‌کند.

## راهنماهای مرتبط

- [معرفی PostgreSQL](../../guide/postgres-fa.md)
- [مرجع فرمان انتقال](../../guide/cli/db-sqlite-to-postgres-fa.md)
- [راهنمای انگلیسی](18-postgres.md)
