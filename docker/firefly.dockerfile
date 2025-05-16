FROM ghcr.io/f1r3fly-io/rnode:latest

# copy genesis to image and make it writable. mounting doesn't work for some reason
COPY --chown=daemon genesis /var/lib/rnode/genesis
