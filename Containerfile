ARG RIPRAP_AGENT_IMAGE
FROM ${RIPRAP_AGENT_IMAGE}

# Replace the OpenCode 1 binary with OpenCode 2; /usr/local/bin/opencode
# execs /opt/opencode/bin/opencode, so replacing it switches the agent.
RUN set -eu; \
    curl -fsSL https://opencode.ai/v2/install | bash -s -- --no-modify-path; \
    mv "$HOME/.opencode/bin/opencode" /opt/opencode/bin/opencode; \
    chmod -R a+rX /opt/opencode; \
    rm -rf "$HOME/.opencode"
