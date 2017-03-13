FROM debian:jessie

RUN groupadd -r symbolserver && useradd -r -g symbolserver symbolserver

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

# grab gosu for easy step-down from root
RUN set -x \
    && export GOSU_VERSION=1.10 \
    && apt-get update && apt-get install -y --no-install-recommends wget && rm -rf /var/lib/apt/lists/* \
    && wget -O /usr/local/bin/gosu "https://github.com/tianon/gosu/releases/download/$GOSU_VERSION/gosu-$(dpkg --print-architecture)" \
    && wget -O /usr/local/bin/gosu.asc "https://github.com/tianon/gosu/releases/download/$GOSU_VERSION/gosu-$(dpkg --print-architecture).asc" \
    && export GNUPGHOME="$(mktemp -d)" \
    && gpg --keyserver ha.pool.sks-keyservers.net --recv-keys B42F6819007F00F88E364FD4036A9C25BF357DD4 \
    && gpg --batch --verify /usr/local/bin/gosu.asc /usr/local/bin/gosu \
    && rm -r "$GNUPGHOME" /usr/local/bin/gosu.asc \
    && chmod +x /usr/local/bin/gosu \
    && gosu nobody true \
    && apt-get purge -y --auto-remove wget

# grab tini for signal processing and zombie killing
RUN set -x \
    && export TINI_VERSION=0.14.0 \
    && apt-get update && apt-get install -y --no-install-recommends wget && rm -rf /var/lib/apt/lists/* \
    && wget -O /usr/local/bin/tini "https://github.com/krallin/tini/releases/download/v$TINI_VERSION/tini" \
    && wget -O /usr/local/bin/tini.asc "https://github.com/krallin/tini/releases/download/v$TINI_VERSION/tini.asc" \
    && export GNUPGHOME="$(mktemp -d)" \
    && gpg --keyserver ha.pool.sks-keyservers.net --recv-keys 6380DC428747F6C393FEACA59A84159D7001A4E5 \
    && gpg --batch --verify /usr/local/bin/tini.asc /usr/local/bin/tini \
    && rm -r "$GNUPGHOME" /usr/local/bin/tini.asc \
    && chmod +x /usr/local/bin/tini \
    && tini -h \
    && apt-get purge -y --auto-remove wget

ENV SYMBOLSERVER_VERSION 1.4.0
ENV SYMBOLSERVER_DOWNLOAD_URL https://github.com/getsentry/symbolserver/releases/download/1.4.0/sentry-symbolserver-Linux-x86_64
ENV SYMBOLSERVER_DOWNLOAD_SHA256 1c588b5ca2df5636bb40374d1f3d3e2187438e70b49b55a08eb215485517c987

RUN set -ex \
    && apt-get update && apt-get install -y --no-install-recommends wget && rm -rf /var/lib/apt/lists/* \
    && wget -O /usr/local/bin/symbolserver "$SYMBOLSERVER_DOWNLOAD_URL" \
    && echo "$SYMBOLSERVER_DOWNLOAD_SHA256  /usr/local/bin/symbolserver" | sha256sum -c - \
    && chmod +x /usr/local/bin/symbolserver \
    && apt-get purge -y --auto-remove wget

ENV SYMBOLSERVER_SYMBOL_DIR /var/lib/symbolserver

RUN mkdir -p $SYMBOLSERVER_SYMBOL_DIR

COPY docker-entrypoint.sh /usr/local/bin/
ENTRYPOINT ["docker-entrypoint.sh"]

EXPOSE 3000
VOLUME /var/lib/symbolserver
CMD [ "symbolserver" ]
