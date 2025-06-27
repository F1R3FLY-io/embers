FROM docker.io/f1r3flyindustries/f1r3fly-scala-node:latest

ARG CHAIN_ID

# copy genesis to image and make it writable. mounting doesn't work for some reason
COPY --chown=daemon ${CHAIN_ID}/genesis /var/lib/rnode/genesis
