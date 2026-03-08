# Deployment Files

`image_resize.service`

- Install to `/etc/systemd/system/image_resize.service`.
- Adjust `User` and `Group` to the VPS service account.
- This unit reads `/opt/image_resize/.env` and starts `/opt/image_resize/image_resize`.

`run_cleanup.sh`

- Place at `/opt/image_resize/run_cleanup.sh`.
- This script loads `/opt/image_resize/.env` and runs `/opt/image_resize/image_resize cleanup`.
- Make it executable with `chmod +x /opt/image_resize/run_cleanup.sh`.

`image_resize.cron`

- Install with `crontab -e` for the same user that runs the application.
- Runs cleanup every day at 12:00 Asia/Tokyo.
- Uses `flock` to avoid overlapping cleanup runs.
