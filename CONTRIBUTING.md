# 🤝 Contributing Guide

Thank you for your interest in OneBrain! Every contribution is valued — from fixing a typo to proposing a new system architecture.

> *"OneBrain believes no contribution is too small — just as no knowledge is without value."*

---

## 📋 Table of Contents

- [How to Contribute](#how-to-contribute)
- [Workflow](#workflow)
- [Branch Naming Conventions](#branch-naming-conventions)
- [Commit Messages](#commit-messages)
- [Pull Requests](#pull-requests)
- [Reporting Bugs](#reporting-bugs)
- [Requesting Features](#requesting-features)
- [Community](#community)

---

## How to Contribute

### 🌟 For Beginners

New to open source? No worries! Here are some easy ways to get started:

1. **⭐ Star** this repository to show your support
2. **📖 Read** the documentation and suggest improvements
3. **🐛 Report bugs** if you find any issues
4. **💡 Suggest ideas** for new features
5. **🌐 Translate** documentation into other languages
6. **📝 Improve** existing documentation

### 🔧 For Developers

1. **Fork** the repository
2. **Clone** it to your local machine
3. Create a new **branch** for your feature or fix
4. **Develop** and write tests
5. Submit a **Pull Request**

---

## Workflow

### 1. Fork & Clone

```bash
# Fork the repo on GitHub, then:
git clone https://github.com/<your-username>/OneBrain.git
cd OneBrain
git remote add upstream https://github.com/onebrain-project/OneBrain.git
```

### 2. Create a Branch

```bash
# Update the main branch
git checkout main
git pull upstream main

# Create a new branch
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
# or
git checkout -b docs/your-doc-update
```

### 3. Develop

- Write clean, well-commented code
- Follow the project's coding style
- Write tests for new code
- Update documentation as needed

### 4. Commit & Push

```bash
git add .
git commit -m "feat: brief description of the change"
git push origin feature/your-feature-name
```

### 5. Open a Pull Request

- Go to GitHub and create a Pull Request from your branch
- Fill in all the details following the PR template
- Wait for a review from the maintainers

---

## Branch Naming Conventions

| Prefix | Purpose | Example |
|---|---|---|
| `feature/` | New feature | `feature/knowledge-graph-api` |
| `fix/` | Bug fix | `fix/voting-calculation` |
| `docs/` | Documentation | `docs/api-reference` |
| `refactor/` | Code refactoring | `refactor/consensus-engine` |
| `test/` | Adding or updating tests | `test/pok-protocol` |
| `chore/` | Maintenance tasks | `chore/update-dependencies` |

---

## Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Types

| Type | Description |
|---|---|
| `feat` | A new feature |
| `fix` | A bug fix |
| `docs` | Documentation changes |
| `style` | Formatting, missing semicolons, etc. (no logic change) |
| `refactor` | Code refactoring (no feature or fix) |
| `test` | Adding or updating tests |
| `chore` | Maintenance, dependency updates |
| `perf` | Performance improvements |

### Examples

```
feat(knowledge-graph): add knowledge unit linking algorithm
fix(voting): correct weighted vote calculation for high-rep users
docs(readme): add BCI integration use case
```

---

## Pull Requests

When creating a PR, please:

1. **Clearly describe** what your changes do
2. **Link related issues** (if any): `Fixes #123`
3. **Include screenshots/recordings** for UI changes
4. **Complete the checklist:**
   - [ ] Code follows the project's coding style
   - [ ] I have reviewed my own code
   - [ ] I have added comments for complex logic
   - [ ] I have updated the relevant documentation
   - [ ] My changes produce no new warnings
   - [ ] I have added tests for my changes
   - [ ] All tests (new and existing) pass

---

## Reporting Bugs

When reporting a bug, please include:

1. **A clear, concise title**
2. **Steps to reproduce** the bug
3. **Expected result** vs. **actual result**
4. **Environment details**: OS, browser, version, etc.
5. **Screenshots**, if possible

---

## Requesting Features

When requesting a feature, please describe:

1. **The problem** the feature would solve
2. **Your proposed solution**
3. **Alternatives** — other solutions you have considered
4. **Additional context** — any background information

---

## Community

### 💬 Contact

- **Email:** shpy2001@gmail.com
- **Discussions:** GitHub Discussions *(coming soon)*
- **Discord:** Coming soon

### 🌍 Language Policy

- **Code and documentation:** English
- **Issues & PRs:** Any language is welcome — write in whatever you're most comfortable with

### 🏆 Recognizing Contributions

All contributors are recognized in [CONTRIBUTORS.md](CONTRIBUTORS.md). True to the spirit of OneBrain — every contribution matters!

---

*Thank you for helping build OneBrain — humanity's shared brain!* 🧠
