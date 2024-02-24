FROM debian:bookworm-slim

COPY ./backend/release/backend /usr/local/bin/mqtt-inspector
COPY ./frontend/wwwroot /srv/mqtt-inspector

CMD ["mqtt-inspector" "/srv/mqtt-inspector"]
