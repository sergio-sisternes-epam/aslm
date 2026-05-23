# ADR-006: Versioning Managed by APM

## Status
Accepted

## Context
SML tags could include a `version` attribute for skill versioning. However, this mixes runtime concerns with packaging concerns.

## Decision
No version attribute in SML syntax. Versioning is managed exclusively by APM.

## Rationale
- Keeps SML syntax lightweight and focused on invocation
- Version resolution is a packaging concern, not a runtime concern
- APM already handles dependency resolution and version management
- Avoids version conflicts between SML tags and installed packages

## Consequences
- SML runtime does not need version comparison logic
- Users must use APM to manage which versions are available
- Reproducibility comes from APM lock files, not SML content
