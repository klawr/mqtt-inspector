FROM debian:bookworm-slim

COPY ./backend/target/release/backend /usr/bin/mqtt-inspector
COPY ./frontend/wwwroot /srv/mqtt-inspector
RUN chmod +x /usr/bin/mqtt-inspector

# TODO implement ctrl-c handling
RUN apt-get update && apt-get install -y tini && rm -rf /var/lib/apt/lists/*

CMD ["/usr/bin/tini", "--", "/usr/bin/mqtt-inspector", "/srv/mqtt-inspector"]