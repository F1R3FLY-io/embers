# Embers Platform Rust Backend Project Guidelines

## Project Context

- This is the Embers Platform Backend - a Rust-based blockchain API server for AI agent deployment and management with integrated wallet functionality.
- This is a Rust workspace project using Poem web framework, Tokio async runtime, and multi-package cargo workspace structure.
- The platform provides HTTP APIs for users to create, edit, and deploy AI agents to blockchain with secure wallet operations.
- **Workspace Structure**: Cargo workspace with 5 packages:
  - `packages/server/` - Main HTTP API server with Poem framework
  - `packages/firefly-client/` - Client library for Firefly blockchain interactions
  - `packages/firefly-client-macros/` - Procedural macros for the client
  - `packages/events-sync/` - Event synchronization service
  - `packages/state-sync/` - State synchronization service
- If the user does not provide enough information with their prompts, ask the user to clarify before executing the task. This should be included in all tasks including writing unit tests, scaffolding the project, as well as implementation of individual modules. In general, follow a test driven development approach whereby unit tests are developed in parallel with the individual components, and features.

## Commands

- Development: `cargo make docker-up` (start development environment)
- Build: `cargo build --all-targets`
- Test: `cargo make test`
- Lint: `cargo make clippy` (uses nightly toolchain)
- Format: `cargo make fmt` (uses nightly toolchain)
- Format Check: `cargo make format-check`
- Security Audit: `cargo make audit`
- Generate Schema: `cargo make generate-schema`
- Run Server: `cargo make server run`
- Python Tests: `pytest tests/` (from project root)
- Python Lint: `ruff check tests/` and `ruff format tests/`
- DO NOT ever `git add`, `git rm` or `git commit, or git push` code. Allow the Claude user to always manually review git changes. `git mv` and `git push` are permitted and inform the developer.
- DO NOT ever remove tests from clippy or type checks.
- Run `cargo make test && cargo build --all-targets` to test code changes before proceeding to a prompt for more instructions or the next task.
- **Operating outside of local repository (with .git/ directory root)**: Not permitted and any file or other operations require user approval and notification

## Code Style

- **Rust Version**: Use stable Rust with nightly for formatting
- **Error Handling**: Use `anyhow` for application errors, `thiserror` for library errors
- **Async**: All async code uses Tokio runtime
- **Imports**: Group imports - std first, external crates next, local imports last
- **Naming**: Follow Rust naming conventions - snake_case for functions/variables, PascalCase for types
- **Documentation**: Document public APIs with doc comments
- **Testing**: Write unit tests in the same file using `#[cfg(test)]` module
- **Clippy**: Code must pass clippy with pedantic and nursery lints enabled
- Follow existing patterns for API endpoints using Poem-OpenAPI
- Follow existing error handling patterns with proper error types
- When adding source code or new files, enhance, update, and provide new unit tests using the existing testing patterns
- If unused variables are required, deliberately prefix them with an \_, underscore
- Maintain comprehensive test coverage
- DO NOT USE emoticons in documentation or the code base

## Best Practices

- Use structured logging with `tracing` crate
- Follow Poem framework patterns for API endpoints
- Implement proper OpenAPI documentation for all endpoints
- Use derive macros where appropriate (serde, thiserror, poem-openapi)
- Maintain modular architecture with clear separation of concerns
- Use proper async/await patterns without blocking the runtime
- Implement proper error propagation with `?` operator
- Use type-safe builders for complex structs
- Follow the existing client-server pattern for blockchain interactions
- Use gRPC with Tonic for inter-service communication

## Testing Best Practices

- Write unit tests in `#[cfg(test)]` modules within source files
- Use Python tests for integration testing of API endpoints
- Mock external dependencies in unit tests
- Test error cases and edge conditions
- Use `cargo make test` to run all Rust tests
- Use `pytest` for Python integration tests
- Ensure tests are deterministic and don't depend on external services
- Write tests that focus on behavior over implementation details
- Use proper async test macros (`#[tokio::test]`)
- Create test fixtures and helpers for common test scenarios

## Project Structure

- Keep the current project structure up to date in the [README.md](./README.md)
- When priming project context on start, read the [README.md](./README.md) as well
- **Workspace Structure**: Using cargo workspace with `packages/` directory
- **Documentation hierarchy**:
  - `docs/` - General documentation and guides
  - `tests/` - Python integration tests
  - `docker/` - Docker configuration files
  - `.github/workflows/` - CI/CD pipeline configuration
- **Main Application** in `packages/server/`:
  - `src/api/` - API endpoint definitions
  - `src/models/` - Data models and types
  - `src/handlers/` - Request handlers
  - `src/utils/` - Utility functions
- **Client Libraries** in `packages/`:
  - `firefly-client/` - Blockchain client implementation
  - `events-sync/` - Event synchronization logic
  - `state-sync/` - State synchronization logic

## Key Features

### API Server (`packages/server/`)

- **AI Agent Management**: CRUD operations for AI agents and teams
- **Wallet Operations**: Token transfers, balance checks, wallet management
- **Testnet Support**: Faucet operations and testnet-specific features
- **OpenAPI Documentation**: Auto-generated API documentation
- **Health Monitoring**: Health check endpoints for service monitoring

### Firefly Client (`packages/firefly-client/`)

- **Blockchain Interactions**: Direct communication with Firefly network
- **Transaction Management**: Submit and monitor blockchain transactions
- **Event Streaming**: Real-time blockchain event monitoring
- **Type Safety**: Strongly typed blockchain operations

## Environment Variables

Required environment variables should be configured appropriately:

- `PORT`: Server port (default: 4000)
- `FIREFLY_READ_NODE`: Read node endpoint for blockchain queries
- `FIREFLY_WRITE_NODE`: Write node endpoint for blockchain transactions
- Additional environment variables as needed for service integrations

## Common Tasks

- Review `git history` to determine how code base evolved or history for particular files and functions
- Use cargo-make tasks for common operations
- Check Docker logs when debugging service issues
- Use `cargo make docker-up` to start the full development environment
- Generate OpenAPI schema with `cargo make generate-schema`
- Run clippy before committing: `cargo make clippy`
- Format code with: `cargo make fmt`
- Check for security vulnerabilities: `cargo make audit`

## Project Specifics

- **Web Framework**: Poem with OpenAPI support for type-safe API development
- **Async Runtime**: Tokio for high-performance async operations
- **Blockchain Integration**: Custom Firefly client with gRPC communication
- **Cryptography**: secp256k1 for elliptic curve operations, blake2/sha3 for hashing
- **Serialization**: Serde for JSON, Prost for Protocol Buffers
- Observe the clippy rules when writing code
- Follow type safety with proper error types
- Use derive macros to reduce boilerplate

## Security Considerations

- **Input Validation**: All API inputs validated with Poem-OpenAPI
- **Rate Limiting**: Implement rate limiting for API endpoints
- **Secure Communication**: Use TLS for all external communications
- **Key Management**: Never log or expose private keys
- Follow security best practices and never introduce code that exposes or logs secrets and keys
- Never commit secrets or keys to the repository
- Use cargo-audit regularly to check for vulnerabilities

## Performance Optimization

- Use async operations throughout to avoid blocking
- Implement proper connection pooling for database/blockchain connections
- Use efficient serialization with Serde
- Profile with cargo flamegraph for performance bottlenecks
- Optimize Docker images with multi-stage builds
- Use cargo build --release for production deployments

## Technical Stack

- **Language**: Rust (stable with nightly formatting)
- **Web Framework**: Poem v3.1 with OpenAPI v5.1
- **Async Runtime**: Tokio v1.47
- **gRPC**: Tonic v0.14
- **HTTP Client**: Reqwest v0.12
- **Cryptography**: secp256k1, blake2, sha3
- **Testing**: Rust built-in testing, Python pytest
- **Build Tools**: Cargo, cargo-make
- **CI/CD**: GitHub Actions
- **Containerization**: Docker with multi-arch support
- **Linting**: Clippy (pedantic + nursery), Ruff for Python

# important-instruction-reminders

Do what has been asked; nothing more, nothing less.
NEVER create files unless they're absolutely necessary for achieving your goal.
ALWAYS prefer editing an existing file to creating a new one.
NEVER proactively create documentation files (\*.md) or README files. Only create documentation files if explicitly requested by the User.