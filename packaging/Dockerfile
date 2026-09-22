# `moonkale-server` in a container — the image that fills the repository's
# **Packages** sidebar (ghcr.io/mathstruct/moonkale-server) and the easiest
# way to self-host a Moonkale server ([[Remote and Server Modes]]).
#
# It does not compile anything: the release workflow's Linux job already
# produced the tarball, and this copies `bin/moonkale-server` out of it.
# Build locally with:
#   packaging/build-release.sh --no-arch
#   docker build -f packaging/Dockerfile --build-arg TARBALL=dist/moonkale-0.1.0-linux-x86_64.tar.gz -t moonkale-server .
#
# Run it (a token is required for any non-loopback bind; TLS or a reverse
# proxy in front — `MOONKALE_INSECURE_HTTP=1` only on a trusted network):
#   docker run --rm -p 8080:8080 -v /srv/notes:/data:rw \
#     -e MOONKALE_TOKEN=... -e MOONKALE_INSECURE_HTTP=1 ghcr.io/mathstruct/moonkale-server
# Ubuntu 24.04 to match the runner the binary is built on (glibc 2.39).
FROM ubuntu:24.04

# So GitHub links the package to this repository and shows a description.
LABEL org.opencontainers.image.source="https://github.com/MathStruct/Moonkale" \
      org.opencontainers.image.description="moonkale-server: the Moonkale server and its browser client — open a folder over HTTP, with terminal, git, language servers and agent sessions on this machine." \
      org.opencontainers.image.documentation="https://mathstruct.github.io/Moonkale/packaging/Install" \
      org.opencontainers.image.licenses="MIT"

ARG TARBALL
COPY ${TARBALL} /tmp/moonkale.tar.gz

# The browser client: the binary looks for ../lib/Moonkale/public next to
# itself (P-126), so /usr/local/bin + /usr/local/lib/Moonkale/public.

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 libstdc++6 libbrotli1 libzstd1 zlib1g git openssh-client \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /tmp/x && tar -xzf /tmp/moonkale.tar.gz -C /tmp/x --strip-components=1 \
 && install -m755 /tmp/x/bin/moonkale-server /usr/local/bin/moonkale-server \
 && mkdir -p /usr/local/lib/Moonkale && cp -r /tmp/x/lib/Moonkale/public /usr/local/lib/Moonkale/public \
 && rm -rf /tmp/x /tmp/moonkale.tar.gz \
 && useradd -r -u 10001 -m -d /home/moonkale moonkale \
 && mkdir -p /data && chown moonkale:moonkale /data

USER moonkale
WORKDIR /data
# The folder the server serves; mount yours over it.
ENV MOONKALE_ROOT=/data
EXPOSE 8080
ENTRYPOINT ["moonkale-server"]
CMD ["--bind", "0.0.0.0", "--port", "8080"]
