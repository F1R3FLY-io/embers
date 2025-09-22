# Embers Documentation Hub

Welcome to the Embers Platform documentation. This repository follows F1R3FLY.io's documentation-first methodology, serving as the central knowledge base for the Embers backend services.

## 📚 Documentation Categories

### Requirements
User stories, business requirements, and acceptance criteria that define what the system should do.

- **[User Stories](./requirements/user-stories/)** - Feature requirements from stakeholder perspective
- **[Business Requirements](./requirements/business-requirements/)** - Business logic and organizational constraints
- **[Acceptance Criteria](./requirements/acceptance-criteria/)** - Definition of done for features

### Specifications
Technical specifications and design documents that define how the system will be built.

- **[Technical Specifications](./specifications/technical/)** - API specs, data schemas, and technical standards
- **[Standards](./specifications/standards/)** - Coding standards and best practices
- **[Integration](./specifications/integration/)** - Third-party service integration specifications

### Architecture
System design and architectural decisions that guide the implementation.

- **[Architecture Decision Records](./architecture/decisions/)** - ADRs documenting key design choices
- **[System Diagrams](./architecture/diagrams/)** - Visual representations of system components
- **[Patterns](./architecture/patterns/)** - Established patterns and conventions

### API Documentation
Comprehensive API reference and usage examples.

- **[OpenAPI Specifications](./api/openapi/)** - Machine-readable API definitions
- **[API Examples](./api/examples/)** - Practical usage examples and tutorials

### Governance
Organizational policies, procedures, and project management.

- **[Policies](./governance/)** - Security, contribution, and operational policies
- **[Current Status](./ToDos.md)** - Live project status and active tasks

## 🔄 Documentation Workflow

### Process Steps

1. **Start with Requirements**
   - Document user stories in `requirements/user-stories/`
   - Define business requirements in `requirements/business-requirements/`
   - Establish acceptance criteria in `requirements/acceptance-criteria/`

2. **Create Specifications**
   - Technical details in `specifications/technical/`
   - Coding standards in `specifications/standards/`
   - Integration specs in `specifications/integration/`

3. **Document Architecture**
   - Record decisions in `architecture/decisions/`
   - Create diagrams in `architecture/diagrams/`
   - Define patterns in `architecture/patterns/`

4. **Implementation**
   - Execute based on approved documentation
   - Reference documentation during development
   - Update documentation with implementation details

5. **Validation**
   - Ensure implementation matches documentation
   - Update documentation with any deviations
   - Document lessons learned

## 📖 For Contributors

When contributing to Embers:

1. **Documentation First**: All changes start with documentation
2. **Follow Templates**: Use provided templates for consistency
3. **Review Process**: All documentation requires pull request review
4. **Stay Current**: Keep documentation updated with code changes

## 🤖 For LLM-Assisted Development

When using AI coding assistants, provide context from:

- **Organization Context**: F1R3FLY.io's CLAUDE.md
- **Project Context**: Embers-specific CLAUDE.md
- **Requirements**: Relevant files from `requirements/`
- **Specifications**: Technical specs from `specifications/`
- **Architecture**: Constraints from `architecture/`
- **Current Tasks**: Priorities from `ToDos.md`

## 📊 Documentation Standards

### File Naming
- Use kebab-case for file names
- Include dates for time-sensitive documents (YYYY-MM-DD format)
- Use clear, descriptive names

### Content Structure
- Start with a clear title and purpose
- Include metadata (author, date, status)
- Use consistent heading hierarchy
- Provide examples where applicable
- Include cross-references to related documents

### Version Control
- Use conventional commits for documentation changes
- Create feature branches for new documentation
- Require reviews for documentation changes
- Maintain documentation alongside code

## 🔗 Quick Links

- [Project Structure](../PROJECT_STRUCTURE.md)
- [Main README](../README.md)
- [CLAUDE Context](../CLAUDE.md)
- [Current Tasks](./ToDos.md)
- [API Documentation](./api/)
- [Architecture Decisions](./architecture/decisions/)

---

*This documentation follows F1R3FLY.io's documentation-first methodology. All features and changes begin with documentation to ensure clear requirements before implementation.*