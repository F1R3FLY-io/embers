# Embers Platform - Project Structure

Following F1R3FLY.io's documentation-first methodology, this document outlines the comprehensive structure and standards for the Embers Platform backend services.

## 📚 Documentation-First Approach

This project follows F1R3FLY.io's documentation-first methodology optimized for both human developers and LLM-assisted development. All features begin with documentation, ensuring clear requirements before implementation.

### Core Documentation Structure

- **📋 [Requirements](./requirements)** - User stories, business requirements, and acceptance criteria
  - `user-stories/` - Feature requirements from user perspective
  - `business-requirements/` - Business logic and constraints
  - `acceptance-criteria/` - Definition of done for features

- **📐 [Specifications](./specifications)** - Technical specifications and design documents
  - `technical/` - API specifications, data schemas, and algorithms
  - `standards/` - Coding standards and best practices
  - `integration/` - Third-party service integration specs

- **🏗️ [Architecture](./architecture)** - System design and architectural decisions
  - `decisions/` - Architecture Decision Records (ADRs)
  - `diagrams/` - System component diagrams and data flows
  - `patterns/` - Established patterns and conventions

- **✅ [Current Status](./ToDos.md)** - Live project status, active tasks, and priorities

## 🗂️ Repository Structure

```
embers/
├── docs/                        # Documentation hierarchy
│   ├── requirements/            # Business and user requirements
│   │   ├── user-stories/        # Feature requirements from stakeholder perspective
│   │   ├── business-requirements/ # Business logic and organizational constraints
│   │   └── acceptance-criteria/ # Definition of done for features
│   ├── specifications/          # Technical specifications
│   │   ├── technical/           # API specifications, data schemas, standards
│   │   ├── standards/           # Coding standards and best practices
│   │   └── integration/         # External service integration specs
│   ├── architecture/            # System design documents
│   │   ├── decisions/           # Architecture Decision Records (ADRs)
│   │   ├── diagrams/            # System component diagrams
│   │   └── patterns/            # Established patterns and conventions
│   ├── api/                     # API documentation
│   │   ├── openapi/             # OpenAPI specifications
│   │   └── examples/            # API usage examples
│   ├── governance/              # Organizational policies and procedures
│   └── ToDos.md                 # Current status and tasks
├── packages/                    # Cargo workspace packages
│   ├── server/                  # Main HTTP API server
│   │   ├── src/
│   │   │   ├── api/             # API endpoint definitions
│   │   │   ├── models/          # Data models and types
│   │   │   ├── handlers/        # Request handlers
│   │   │   └── utils/           # Utility functions
│   │   └── README.md            # Package-specific documentation
│   ├── firefly-client/          # Firefly blockchain client
│   │   ├── src/
│   │   └── README.md
│   ├── firefly-client-macros/   # Procedural macros
│   │   ├── src/
│   │   └── README.md
│   ├── events-sync/             # Event synchronization service
│   │   ├── src/
│   │   └── README.md
│   └── state-sync/              # State synchronization service
│       ├── src/
│       └── README.md
├── tests/                       # Python integration tests
│   ├── integration/             # Integration test suites
│   ├── fixtures/                # Test fixtures and data
│   └── README.md
├── docker/                      # Docker configuration
│   ├── dev/                     # Development environment
│   └── prod/                    # Production configuration
├── scripts/                     # Build and utility scripts
├── .github/                     # GitHub configuration
│   └── workflows/               # CI/CD pipelines
├── CLAUDE.md                    # LLM assistant context
├── PROJECT_STRUCTURE.md         # This file
├── README.md                    # Main project overview
├── Cargo.toml                   # Workspace configuration
└── Makefile.toml                # cargo-make tasks
```

## 🔄 Development Workflow

Following F1R3FLY.io standards:

1. **📖 Documentation First**
   - Start with requirements in `docs/requirements/`
   - Create/update technical specs in `docs/specifications/`
   - Document architectural decisions in `docs/architecture/decisions/`

2. **🤖 LLM Integration**
   - Provide comprehensive context from CLAUDE.md
   - Reference documentation for project-specific instructions
   - Update documentation alongside code changes

3. **⚙️ Development Standards**
   - Use cargo-make for consistent task execution
   - Follow test-driven development (TDD) practices
   - Maintain comprehensive test coverage
   - Use conventional commits for version control
   - Implement CI/CD checks before merging

4. **📝 Continuous Documentation**
   - Keep `docs/ToDos.md` updated with current status
   - Update relevant documentation with each PR
   - Maintain README files at package levels

## 🛠️ Technical Stack

- **Language**: Rust (stable with nightly formatting)
- **Web Framework**: Poem v3.1 with OpenAPI v5.1
- **Async Runtime**: Tokio v1.47
- **gRPC**: Tonic v0.14
- **Testing**: Rust unit tests + Python integration tests (pytest)
- **Build Tools**: Cargo, cargo-make
- **Version Control**: Git with documentation-first branching
- **CI/CD**: GitHub Actions with multi-arch Docker support
- **Documentation**: Markdown with GitHub integration

## 📊 Project Governance

### Documentation Standards

Following F1R3FLY.io's methodology:
- All changes start with documentation
- Use templates from organizational standards
- Require pull request reviews for documentation updates
- Maintain consistency across all F1R3FLY.io projects

### Branching Strategy

- `main` - Production-ready code
- `develop` - Integration branch
- `docs/*` - Documentation changes
- `feature/*` - New features (start with docs)
- `fix/*` - Bug fixes
- `refactor/*` - Code refactoring

### Commit Standards

Use conventional commits:
- `docs:` - Documentation changes
- `feat:` - New features
- `fix:` - Bug fixes
- `refactor:` - Code refactoring
- `test:` - Test additions/modifications
- `chore:` - Maintenance tasks

## 🔐 Security Considerations

- Input validation with Poem-OpenAPI
- Rate limiting for API endpoints
- Secure communication with TLS
- Key management best practices
- Regular security audits with cargo-audit
- No secrets in repository

## 📈 Performance Standards

- Async operations throughout
- Connection pooling for external services
- Efficient serialization with Serde
- Performance profiling with flamegraph
- Optimized Docker multi-stage builds
- Target sub-100ms API response times

## 🤝 Contributing

Please follow F1R3FLY.io's contribution standards:

1. Start with documentation in `docs/requirements/`
2. Create technical specifications
3. Document architectural decisions
4. Implement based on approved documentation
5. Ensure tests pass and coverage is maintained
6. Update relevant documentation
7. Submit pull request for review

## 📄 License

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)

## 🙏 Acknowledgments

- F1R3FLY.io organization for documentation standards
- Rust community for excellent tooling
- Contributors and maintainers

---

**F1R3FLY.io Project**: This project is part of the F1R3FLY.io ecosystem, committed to advancing distributed computing and blockchain technology through open source collaboration and documentation-first development.