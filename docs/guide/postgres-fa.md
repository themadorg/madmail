# PostgreSQL

Madmail v2 می‌تواند حساب‌ها، تنظیمات، سهمیه‌ها، توکن‌ها و وضعیت فدراسیون را به‌جای SQLite در **PostgreSQL** نگه دارد. نامه‌ها در Maildir می‌مانند. **MySQL فعلا پشتیبانی نمی‌شود.**

`madmail install` همچنان SQLite را تنظیم می‌کند. برای پایگاه PostgreSQL تازه، `driver` و `dsn` را تغییر دهید و سرویس را دوباره راه‌اندازی کنید. برای انتقال داده‌های موجود، ابتدا Madmail را متوقف کنید و از فرمان `madmail db sqlite-to-postgres` استفاده کنید؛ تغییر تنظیمات به‌تنهایی داده‌ها را کپی نمی‌کند.

- [نصب با Docker یا مدیر بسته و انتقال داده‌ها](../project/user-guide/18-postgres-fa.md)
- [مرجع فرمان انتقال](cli/db-sqlite-to-postgres-fa.md)
- [نسخهٔ انگلیسی](postgres.md)
