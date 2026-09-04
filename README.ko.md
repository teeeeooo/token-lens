<p align="right">
  <a href="./README.md">English</a> | <strong>한국어</strong>
</p>

# Token Lens

**Codex, Claude Code, Antigravity의 토큰 사용량과 quota를 확인하기 위한 Windows용 보안 강화 데스크톱 모니터입니다.**

Token Lens는 [Javis603/token-monitor](https://github.com/Javis603/token-monitor)를 기반으로 만든 비공개 downstream 배포판입니다. 업스트림의 대시보드와 사용량 수집 장점은 유지하면서, 로컬 Windows 사용 환경에 맞춰 실행 기능과 보안 표면을 의도적으로 줄였습니다.

> Token Lens는 업스트림 Token Monitor의 단순 미러가 아닙니다. 지원하지 않는 provider와 일부 계정 관리, 동기화, updater, credential 관리 기능은 의도적으로 비활성화하거나 UI에서 숨겼습니다.

## 왜 이 포크를 만들었나

업스트림은 매우 다양한 AI 도구와 multi-device 기능을 지원합니다. Token Lens는 대신 다음 운영 모델에 집중합니다.

- **Codex, Claude Code, Antigravity만 지원**
- 모델명을 하드코딩하지 않는 모델별 토큰 집계
- 로컬 사용량 history, quota/reset 확인, floating desktop UI
- provider credential은 원래의 코딩 도구가 계속 소유
- Token Lens 자체 account switching / credential 저장 없음
- multi-device Hub/sync, Discord RPC, 앱 자동 업데이트, runtime tokscale 다운로드 없음
- transcript 노출 최소화: 세션 상세에는 사용량 metadata만 유지하고 prompt/response 내용은 제외

즉 Windows 개인 사용, 특히 불필요한 공격 표면을 최소화하고 싶은 관리형/회사 PC 환경을 위한 더 작고 보수적인 downstream contract를 목표로 합니다.

## 지원 도구

| 도구 | 토큰 사용량 | Quota / Limits | 세션 metadata |
|---|:---:|:---:|:---:|
| Claude Code | ✅ | ✅ | ✅ |
| Codex | ✅ | ✅ | ✅ |
| Antigravity | ✅ | ✅ | — |

### 모델 처리 방식

Token Lens는 모델 ID가 아니라 **provider를 제한**합니다. 따라서 Codex, Claude Code, Antigravity에 새 모델이 추가되어도 해당 provider의 로그/API schema가 호환되는 한 새 모델명은 모델별 사용량 집계에 그대로 나타납니다.

가격 정보가 아직 없는 새 모델도 토큰 수 자체는 집계할 수 있습니다.

## 보안 모델

Token Lens는 인증 관리자가 아니라 **모니터**입니다.

현재 downstream 보안 계약은 다음과 같습니다.

- runtime provider allowlist: `claude`, `codex`, `antigravity`
- Hub / 원격 동기화 비활성화
- embedded Hub listener 비활성화
- Discord Rich Presence 비활성화
- 앱 자동 업데이트 비활성화
- runtime tokscale 다운로드/업데이트 비활성화
- Token Lens 자체 credential store 없음
- Codex 인증 상태 read-only
- Claude Code 인증 상태 read-only, Token Lens가 OAuth credential을 refresh/rotate하지 않음
- Token Lens-managed Antigravity OAuth account 없음
- session parser 단계에서 prompt preview 비활성화
- renderer session detail payload에 prompt/response text 포함 금지
- 부가적인 third-party network 기능 비활성화
- 업스트림의 Electron isolation/CSP/navigation 제한 유지
- Windows runtime/package identity 분리: `Token Lens` / `com.teeeeooo.tokenlens`

이 보안 계약은 downstream CI와 Windows packaging 전에 invariant suite로 검증됩니다.

자세한 내용은 [SECURITY.md](./SECURITY.md)를 참고하세요.

## 인증 동작

Token Lens는 quota 조회에 필요한 경우에도 각 코딩 도구가 이미 관리하고 있는 인증/세션 상태를 읽는 방식으로 동작합니다.

- **Codex:** Codex CLI에서 정상적으로 로그인합니다. Token Lens는 기존 상태를 읽기만 하며 `auth.json`을 수정하거나 로그인/계정 전환을 수행하지 않습니다.
- **Claude Code:** Claude Code에서 정상적으로 인증합니다. Token Lens는 유효한 기존 OAuth/CLI 상태를 읽을 수 있지만 cookie를 저장하거나 OAuth credential을 직접 갱신하지 않습니다.
- **Antigravity:** 기존 로컬 Antigravity state/RPC가 있으면 이를 사용하며 Token Lens 자체 계정을 만들지 않습니다.

provider 인증이 만료되면 해당 코딩 도구가 자신의 인증을 다시 갱신할 때까지 quota가 unavailable로 표시될 수 있습니다.

## 데이터 / Privacy 경계

Token Lens는 지원하는 세 도구의 로컬 usage/session 파일을 읽습니다. 이 파일에는 민감한 개발 정보가 포함될 수 있습니다.

Claude Code와 Codex의 세션 상세에서는 timestamps, token/cache count, tool name, turn 정보, cost attribution 등 사용량 분석에 필요한 metadata만 유지합니다. `promptPreview`는 비어 있고 prompt/response text는 반환되는 session-detail record나 renderer로 전달되지 않습니다.

단, 이러한 metadata를 계산하기 위해 raw transcript 파일 자체는 로컬 프로세스에서 파싱됩니다. 따라서 Token Lens는 여전히 권한이 높은 로컬 개발 도구로 취급해야 합니다.

## Windows 설치

Token Lens는 현재 GitHub Actions에서 Windows x64 artifact를 생성합니다.

- `Token-Lens-Setup-<version>.exe` — 설치형
- `Token-Lens-<version>.exe` — portable
- `SHA256SUMS.txt` — artifact checksum

저장소의 **Actions → Build Token Lens Windows**에서 성공한 `main` workflow run을 열고 `token-lens-windows-x64` artifact를 다운로드하면 됩니다.

### 서명 상태

Token Lens downstream artifact는 의도적으로 **unsigned**입니다. 업스트림 프로젝트의 SignPath identity를 사용하지 않습니다.

따라서 Windows SmartScreen, WDAC/AppLocker, EDR 또는 회사 보안 정책에서 경고하거나 실행을 차단할 수 있습니다. 관리형 PC에서는 회사의 software/code-signing 정책을 따라야 합니다.

## 현재 upstream baseline

Token Lens는 현재 upstream v0.53.0 code line을 기반으로 하며 최초 import 기준은 다음과 같습니다.

```text
Javis603/token-monitor
v0.53.0
0b17b1ec53ccd60508a645144ccb7db74027168c
```

관련 provider patch를 선택적으로 가져올 수 있도록 upstream baseline의 Git history를 보존하고 있습니다. Token Lens를 독립 재작성 프로젝트로 만들지 않는 이유도 upstream 유지보수 비용을 낮추기 위해서입니다.

자세한 업데이트 절차는 [UPSTREAM.md](./UPSTREAM.md)를 참고하세요.

## 개발

필요 환경:

- Node.js 22
- npm
- 지원되는 Windows package artifact 생성 시 Windows 환경

dependency 설치:

```bash
npm ci --prefer-offline --no-audit --no-fund
```

downstream policy 적용:

```bash
node scripts/downstream/apply-hardening.js
node scripts/downstream/apply-provider-hardening.js
node scripts/downstream/apply-renderer-hardening.js
```

downstream invariant 및 lint:

```bash
node --test tests-downstream/security-invariants.test.js tests-downstream/branding-invariants.test.js
npm run lint
```

production dependency audit:

```bash
npm audit --omit=dev --audit-level=high
```

GitHub Actions에서도 Windows installer/portable artifact를 만들기 전에 동일한 핵심 검증을 수행합니다.

## Upstream과의 관계

Token Lens는 MIT 라이선스의 [Javis603/token-monitor](https://github.com/Javis603/token-monitor)를 기반으로 하며 상당한 업스트림 코드와 Git history를 유지합니다.

다만 downstream의 실행 계약은 의도적으로 더 좁습니다. 업스트림 변경은 자동 수용하지 않고 다음 영역을 중심으로 검토한 후 통합합니다.

- Codex / Claude Code / Antigravity collector와 quota API
- credential 및 authentication 동작
- Electron/preload/IPC 보안 경계
- network listener와 동기화 기능
- tokscale packaging/update 동작
- 신규 모델 및 log schema 변화

업스트림의 workflow는 downstream release 경로에 자동으로 신뢰하거나 가져오지 않습니다.

## 라이선스

MIT. [LICENSE](./LICENSE)를 참고하세요.

Token Lens는 비공식 downstream 프로젝트이며 업스트림 maintainer, OpenAI, Anthropic 또는 Google과 제휴하거나 공식적으로 보증받은 프로젝트가 아닙니다.
