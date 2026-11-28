FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
LABEL org.opencontainers.image.source="https://github.com/beyond10x/spindle"
LABEL org.opencontainers.image.description="b10x-spindle, the Spindle runtime for ESS specifications"
LABEL org.opencontainers.image.licenses="Apache-2.0"
COPY --chmod=0555 b10x-spindle /b10x-spindle
USER 65532:65532
ENTRYPOINT ["/b10x-spindle"]
