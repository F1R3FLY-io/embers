# Embers Platform

> F1R3Sky wallets and AI agents backend services - Rust-based blockchain API server for AI agent deployment and management with integrated wallet functionality.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)

## 🚀 Quick Start

```bash
# Start development environment
cargo make docker-up

# Run the server
cargo make server run

# Run tests
cargo make test
```

## 📁 Project Structure

```
embers/
├── docs/                        # 📚 Documentation hub
│   ├── requirements/            # User stories & business requirements
│   ├── specifications/          # Technical specs & standards
│   ├── architecture/            # System design & ADRs
│   ├── api/                     # API documentation
│   └── ToDos.md                 # Current status & tasks
├── packages/                    # Cargo workspace packages
│   ├── server/                  # Main HTTP API server (Poem)
│   ├── firefly-client/          # Firefly blockchain client
│   ├── firefly-client-macros/   # Procedural macros
│   ├── events-sync/             # Event synchronization service
│   └── state-sync/              # State synchronization service
├── tests/                       # Python integration tests
├── docker/                      # Docker configuration
├── scripts/                     # Build and utility scripts
├── .github/workflows/           # CI/CD pipelines
├── CLAUDE.md                    # LLM assistant context
└── Makefile.toml                # cargo-make tasks
```

## 📚 Documentation

This project follows F1R3FLY.io's **documentation-first methodology**. All features begin with documentation before implementation.

- **[📋 Project Structure](./docs/PROJECT_STRUCTURE.md)** - Comprehensive project organization and standards
- **[📖 Documentation Hub](./docs/README.md)** - Complete documentation guide
- **[✅ Current Status](./docs/ToDos.md)** - Live project status and active tasks
- **[🤖 LLM Context](./CLAUDE.md)** - AI assistant instructions and context

## 🛠️ Tech Stack

- **Rust** - Systems programming language
- **Poem** - Modern HTTP web framework with OpenAPI
- **Tokio** - Async runtime
- **Tonic** - gRPC framework
- **Python** - Integration testing (pytest)

## 🤝 Contributing

Please follow F1R3FLY.io's standards:

1. Start with documentation in `docs/requirements/`
2. Create technical specifications in `docs/specifications/`
3. Document architectural decisions in `docs/architecture/decisions/`
4. Implement based on approved documentation

See [Contributing Guidelines](./docs/README.md#for-contributors) for details.

## 🔗 Related Projects

- [embers-frontend](https://github.com/F1R3FLY-io/embers-frontend) - Web interface
- [f1r3fly](https://github.com/F1R3FLY-io/f1r3fly) - Core transaction server
- [f1r3sky](https://github.com/F1R3FLY-io/f1r3sky) - Social platform

---

**Part of the [F1R3FLY.io](https://github.com/F1R3FLY-io) ecosystem** - Advancing distributed computing and blockchain technology through documentation-first development.
