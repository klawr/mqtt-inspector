FROM debian:bookworm-slim

COPY ./backend/target/release/backend /usr/bin/mqtt-inspector
COPY ./wwwroot /srv/mqtt-inspector
RUN chmod +x /usr/bin/mqtt-inspector
RUN mkdir /srv/config

CMD ["/usr/bin/mqtt-inspector", "/srv/mqtt-inspector", "/srv/config"]