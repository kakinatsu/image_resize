# Deployment Files

`image_resize.service`

- Install to `/etc/systemd/system/image_resize.service`.
- Adjust `User` and `Group` to the VPS service account.
- This unit reads `/opt/image_resize/.env` and starts `/opt/image_resize/image_resize`.
- It grants `CAP_NET_BIND_SERVICE`, so the app can bind to low ports such as `80` while still running as a non-root user.

`run_cleanup.sh`

- Place at `/opt/image_resize/run_cleanup.sh`.
- This script loads `/opt/image_resize/.env` and runs `/opt/image_resize/image_resize cleanup`.
- Make it executable with `chmod +x /opt/image_resize/run_cleanup.sh`.
- Create `/opt/image_resize/logs` and ensure it is writable by the service user.

`image_resize.cron`

- Install with `crontab -e` for the same user that runs the application.
- Runs cleanup every day at 12:00 Asia/Tokyo.
- Uses `flock` to avoid overlapping cleanup runs.
- Writes cleanup output to `/opt/image_resize/logs/cleanup.log`.

## Production Notes

- This application serves plain HTTP and is expected to sit behind Cloudflare.
- In production, set `APP_ADDR=0.0.0.0:80` in `/opt/image_resize/.env`.
- Set `PUBLIC_BASE_URL` to the public HTTPS URL served through Cloudflare.
- Do not set `APP_ADDR` to `:443` unless the application itself is changed to terminate TLS.
