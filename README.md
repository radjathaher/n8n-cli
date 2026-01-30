# n8n-cli

Auto-generated n8n CLI from the OpenAPI schema. Built for scripting and LLM discovery.

## Install

### Install script (macOS arm64 + Linux x86_64)

```bash
curl -fsSL https://raw.githubusercontent.com/radjathaher/n8n-cli/main/scripts/install.sh | bash
```

### Nix (binary fetch)

```bash
nix profile install github:radjathaher/n8n-cli
```

### Build from source

```bash
cargo build --release
./target/release/n8n --help
```

## Auth

```bash
export N8N_API_KEY="n8n_api_..."
export N8N_BASE_URL="https://n8n.example.com"
```

## Discovery

```bash
n8n list --json
n8n describe user get-users --json
n8n tree --json
```

## Examples

List users:

```bash
n8n user get-users --pretty
```

Get a user by ID or email:

```bash
n8n user get-user --id "user@example.com" --pretty
```

Create users (array body):

```bash
n8n user create-user --body '[{"email":"user@example.com","role":"global:member"}]' --pretty
```

Change role:

```bash
n8n user change-role --id "user@example.com" --input-new-role-name "global:member"
```

## Update command tree

```bash
cargo run --bin gen-command-tree -- --in n8n-api.yaml --out schemas/command_tree.json
```
