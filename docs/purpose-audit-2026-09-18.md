# Token Lens v2 — 목적 기반 검수 및 후속 작업 명세

- 검수일: 2026-09-18 (Asia/Seoul)
- 검수 기준: `main` / `4bf1e428483c5d3b055029d654e7fc80df6ff272`
- 직접 수정 커밋: `fb861e7d8e06a0900327d691701fc484239fadb1`
- 상태: A1–A4 및 B1–B4 구현 완료. B5의 제어 가능한 DOM 통합 검증 완료; 실제 Windows/계정/패키지 시각 검증은 별도 게이트로 유지.
- 문서 성격: 이번 검수의 근거와 실행 명세. 장기 제품 계약은 `architecture/v2-architecture.md`, 현재 상태는 `../STATE.md`가 우선한다.

## 1. 결론과 제품 목적

**현재 구조를 유지하고 정확성·실패 격리·자원 사용을 보강하는 것이 맞다. 전면 재설계는 필요하지 않다.**

Token Lens는 AI 코딩 도구의 실제 사용량, 비용, 현재 계정의 한도와 초기화 시점을 확인하는 가벼운 로컬 데스크톱 모니터다. tokScale을 우선 사용하고, 확인된 공백만 좁은 Rust 어댑터로 보완한다.

유지해야 할 범위:

- Codex / Claude Code / Gemini CLI / Antigravity의 사용량·한도·모델·세션 메타데이터.
- 현재 Tauri 2 + Rust 구조, 안정된 정규화 계약, 검증된 대시보드·트레이·bubble UX.
- provider-owned 인증 상태의 최소 읽기와 이미 승인된 CLI 복구 방식.
- provider-owned 제목과 프로젝트 basename만 표시하는 세션 개인정보 경계.
- pinned tokScale 배포, 런타임 다운로드·자동 업데이트 금지.

이번 작업의 비목표: Electron 복귀, 제공사 추가/삭제, 로그인·계정 관리, transcript 뷰어, Hub/동기화, UI 전면 변경, Port Lens 드래그 구현 이식.

## 2. 검수 범위와 증거 수준

`AGENTS.md`, `STATE.md`, 문서 지도, 아키텍처, README, 프런트엔드 정규화·캐시·컨트롤러, Rust 명령/어댑터 경계, 세션 처리, Tauri CSP/capability, 테스트·빌드 설정을 검토했다.

실제 제공사 인증 파일이나 대화 원문을 조회하는 live 테스트는 실행하지 않았다. 아래 재현은 합성 데이터와 가짜 지연/실패, 테스트용 프로세스로 수행했다. 이것은 정적·자동화 검수이며 Windows 실기기 전체 인증이나 완전한 보안 감사를 의미하지 않는다.

## 3. 직접 수정한 항목

| ID | 우선순위 | 문제 / 사용자 영향 | 수정 위치 | 상태 |
|---|---|---|---|---|
| A1 | P1 | 미제공 잔여율이 0%로 표시되거나 사용률 기반 보정이 누락됨 | `src/renderer-model.js` | 수정·회귀 검증 |
| A2 | P1 | 늦게 끝난 과거 조회가 수동 새로고침 결과를 캐시에 덮어씀 | `src/stats-compat.js::cached` | 수정·회귀 검증 |
| A3 | P1 | 과거 인증 복구 결과가 더 최근의 전체 quota 결과를 덮어씀 | `src/stats-compat.js::loadQuotaReport` | 수정·회귀 검증 |
| A4 | P1 | tokScale 실패 시 원문 출력·경로가 renderer 오류로 전달될 수 있음 | `src-tauri/src/tokscale.rs` | 수정·회귀 검증 |

### A1. 미제공 값과 실제 소진을 구분

기존 `windowPercent()`는 `Number(null)`을 0으로 변환했다. Rust/facade가 미제공 수치를 `null`로 정규화하므로, undefined만 사용하는 기존 단위 테스트로는 놓칠 수 있었다.

변경: `optionalFinite()`로 null·undefined·빈 문자열·공백·비유한 수치를 미제공으로 처리한다. 잔여율이 있을 때만 우선 사용하고, 없으면 제공된 사용률로 `100 - usedPercent`를 계산한다. 둘 다 없으면 null을 유지한다. 실제 0은 보존한다.

회귀 검증: 정규화 payload → quotaRows → Home/bubble 경로에서 미제공 quota가 가짜 0%가 되지 않는다. 사용률 25%만 있는 경우 잔여 75%, 실제 잔여 0%는 0%다. 금액만 있는 uncapped spend는 기존 표현을 유지한다.

### A2. 요청 순서와 캐시 소유권 보장

기존 강제 요청은 in-flight 추적 밖에 있었고, 모든 완료 요청이 캐시를 쓸 수 있었다. 이제 일반 요청도 진행 중인 최신 강제 요청을 공유하며, 현재 in-flight 요청만 캐시를 기록하거나 해제한다.

회귀 검증: 과거 10 → 최신 20 완료 → 과거 지연 완료 순서에서도 캐시는 20이다. 최신 요청 실패 후 과거 성공이 도착해도 과거 결과를 새 캐시로 승격하지 않는다. 시계가 역행하면 음수 캐시 나이를 유효한 TTL로 인정하지 않는다.

### A3. 전체 quota와 복구 조회의 세대 구분

전체 quota가 바뀌면 과거 복구 in-flight의 캐시 기록 권한을 무효화한다. 복구 성공·실패 처리 전에 시작 시점의 전체 캐시 객체가 아직 현재 것인지 검사한다. 최근 전체 조회의 healthy 80%를 과거 recovery 결과가 덮지 못한다.

5분 전체 quota / 30초 provider 복구 주기를 유지한다. 복구 결과로 전체 조회의 원래 캐시 시각을 연장하지 않는다. Rust의 인증 재시도·cooldown·provider 권한 판정은 변경하지 않았다.

### A4. 오류 경로에도 개인정보 경계를 적용

기존 tokScale 실행 실패는 stderr 전체, 실행 경로, JSON 파싱 실패 시 stdout 앞 240자를 오류 문자열에 포함할 수 있었다. 정상 payload가 정규화되더라도 오류 경로를 통해 원문이 renderer로 넘어갈 수 있는 구조였다. 실제 개인정보 유출이나 외부 전송을 관측했다는 뜻은 아니다.

변경: stderr는 수집하지 않고 폐기한다. 시작/대기 실패는 OS 오류 종류, 비정상 종료는 종료 상태만 반환한다. JSON/UTF-8 오류는 고정 문구로 반환한다. 원문을 먼저 포함한 뒤 정규식으로 가리는 방식은 사용하지 않는다.

회귀 검증: 가짜 prompt·경로·Bearer 문자열이 들어간 파싱 실패, 존재하지 않는 테스트 실행 경로, 가짜 stderr를 쓰고 종료하는 프로세스가 renderer용 오류에 원문을 포함하지 않는다. 프로세스 stderr 테스트는 Unix 전용이며 Windows에서는 조건부 제외된다.

## 4. 후속 작업의 우선순위

| 순서 | ID | 우선순위 | 작업 | 근거 수준 |
|---|---|---|---|---|
| 1 | B1 | P1 | 사용량·quota 실패 격리와 부분 갱신 | JS 합성 실패 재현 + Rust 제어 흐름 확인 |
| 2 | B2 | P2 | 상태·복구·수집 시각을 명시적 필드로 표현 | 문자열 판정과 공통 시각 대입 코드 확인 |
| 3 | B3 | P2 | 동적 캐시 크기와 수명 제한 | Map 키/삭제 경로 확인; 실제 메모리 피해 미측정 |
| 4 | B4 | P2 | 세션 메타데이터 요청 범위·batch 제한 | 프런트엔드 전체 참조 집합과 Rust 5,000개 제한 확인 |
| 5 | B5 | 검증 게이트 | 실제 Windows·제공사·패키지 회귀 | 기존 미완료 live 게이트 유지; 새 결함 확정 아님 |

B1~B4는 아래의 수용 기준을 통과하는 별도 작은 변경으로 진행한다. 이번에 통합 수정까지 하지 않은 이유는 표시/캐시의 국소 수정과 달리 자원별 상태 계약, provider authority, UI 갱신 흐름을 함께 검증해야 하기 때문이다.

## 5. B1 — 실패 격리와 부분 갱신

### 현재 문제와 근거

`src/stats-compat.js::getStats()`는 Today → Month → All Time → derived → quota를 순차 await한다. Month가 실패하면 quota까지 도달하지 못한다. 합성 실행에서도 호출은 `[usage:today, usage:month]`에서 종료됐고 quota는 호출되지 않았다.

`src/main.js::refresh()`는 완성된 nextStats를 받은 후에만 상태를 교체한다. 따라서 마지막 단계의 quota 실패도 새로 수집한 사용량 표시를 막을 수 있다.

`src-tauri/src/commands.rs::get_quota_report()`는 `adapter.quota_report().await?` 뒤에 독립 제공사 보강을 시작한다. tokScale 명령/파싱 실패가 Gemini·AGY 등 별도 수집 경로의 실행까지 막는 구조다.

### 구현 순서

1. loader에 자원별 결과를 구분할 최소 계약을 추가한다. 자원은 Today, Month, All Time, 현재 derived, quota, history다. 각각 loading/ready/stale/unavailable, 마지막 성공 시각, 정제된 오류를 갖는다.
2. 성공한 자원만 부분 반영한다. 실패한 자원은 마지막 성공 값을 stale로 유지하거나, 성공 이력이 없으면 unavailable로 표시한다. 실패를 사용량 0으로 대체하지 않는다.
3. quota 갱신의 실행 여부를 사용량 조회 성공에 종속시키지 않는다. 초기 Today 우선 표시를 유지하고, 느린 이력 스캔을 무조건 전부 병렬화하지 않는다.
4. Rust quota 오케스트레이션은 tokScale 실패와 개별 제공사 실패를 구분한다. 실패 시 허용된 독립 어댑터가 실행될 수 있도록 base 결과와 오류를 분리하고, 제공사별 결과만 병합한다.
5. Codex는 보고서에 provider row가 없으면 현재 enrichment가 바로 반환한다. 빈 보고서 전달만으로 복구된다고 가정하지 않는다. Codex row 초기화와 account/workspace 확인을 별도 설계하고 기존 일치 검사들을 보존한다.
6. 새 부분 결과도 자원별 세대를 검증한다. A2/A3의 요청 소유권 검사를 유지하여 이전 요청, 다른 기간, 이전 계정 결과가 새 상태를 덮지 못하게 한다.
7. 구현과 함께 `v2-architecture.md`의 갱신 계약과 `STATE.md`를 갱신한다. 고정 facade에 불필요한 광범위 API를 추가하지 않는다.

### 수용 기준

- Today 성공 + Month 실패: Today는 표시되고, quota 조회/표시도 독립적으로 진행된다.
- usage 정상 + quota 실패: 새 usage는 표시되고 quota만 stale/unavailable이다.
- tokScale quota 실패 + 독립 어댑터 성공: 성공한 제공사 한도는 남고 다른 제공사의 값으로 대체되지 않는다.
- 최초 실패와 이전 성공 후 실패를 구분한다. 화면에 0 사용량·0%를 성공 결과처럼 만들지 않는다.
- 지연, timeout, 복구, 강제 갱신이 겹쳐도 자원별 최신 요청이 이긴다.
- 기존 5분 전체 갱신, 30초 복구 확인, Retry-After, 동일 거부 credential 재조회 금지를 유지한다.
- UI 부분 갱신이 provider 필터/scroll 위치를 불필요하게 초기화하지 않는다.

### 위험과 중단 조건

단순 `Promise.all()` 전환만으로 완료 처리하지 않는다. 실패 격리, 디스크 스캔 병렬 수, 최신 결과 우선권은 서로 다른 문제다. account/workspace 확인을 생략해야만 fallback이 동작한다면 그 fallback을 배포하지 않는다.

## 6. B2 — 구조화된 상태와 정확한 수집 시각

### 현재 문제와 근거

`commands.rs::provider_auth_recovery_pending()`과 `stats-compat.js::quotaAuthRefreshPending()`가 영어 diagnostic 문구를 검색한다. `quotaReportToCompatLimits()`도 `Stale ` 접두어로 stale 여부를 결정한다. 사람이 읽는 문구 변경이 복구 스케줄의 의미를 바꿀 수 있다.

provider의 `updatedAt`에 공통 보고서 생성 시각을 넣고, provider 복구 명령은 일부만 조회해도 보고서 생성 시각을 갱신한다. `main.js::mergeStatsPatch()`도 현재 시간을 포함해 갱신 시각을 만든다. 보고서 조립 시각과 실제 개별 수집 시각을 구분할 필요가 있다.

주의: Home/상세 Limits에는 이미 stale 배지가 있다. bubble도 현재 stale 공급자를 제외한다. 이 작업은 stale 표시를 처음 추가하는 것이 아니라 판정 근거와 시각을 정확하게 만드는 것이다.

### 제안 계약과 구현

- Rust 정규화 provider에 `status: ready | stale | unavailable`, `recoveryState: idle | pending | cooldown`, `lastSuccessAtMs`, `lastAttemptAtMs`, `retryAtMs`의 최소 필드를 검토한다. 명칭은 구현 시 도메인 규칙과 통일한다.
- diagnostic은 설명 전용으로 만든다. 동작은 enum/시각 필드로 결정한다. credential·fingerprint·원문 payload는 새 필드에도 포함하지 않는다.
- 보고서 생성 시각은 유지해도 되지만 provider 수집 성공 시각으로 사용하지 않는다. 복구 실패와 cached 재표시는 마지막 성공 시각을 연장하지 않는다.
- 복구 과정의 이전 계정 snapshot 재사용 여부는 별도 계정 전환 테스트로 확인한다. 확인 전에는 다른 계정으로 안전하게 재사용 가능하다고 가정하지 않는다.
- facade/renderer를 단계적으로 이관하며 정상·stale·미제공 표현을 보존한다. 현재 bubble stale 제외 정책은 이번 구조화만으로 바꾸지 않는다.

### 수용 기준

1. diagnostic을 다른 문구/언어로 변경해도 polling 대상·주기·status가 같다.
2. Gemini만 재수집해도 Codex/Claude/AGY의 `lastSuccessAtMs`가 바뀌지 않는다.
3. stale 재표시와 실패 재시도가 마지막 성공 시각이나 stale 허용 기간을 연장하지 않는다.
4. reset 시각이 지난 quota는 stale fallback의 유효값으로 재사용하지 않는다.
5. 화면에 “방금 갱신”을 표시한다면 자원의 실제 수집 근거가 있어야 한다. 표시가 필요하지 않으면 무리하게 새 UI를 추가하지 않는다.
6. 기존 429 보호와 동일 credential 재조회 금지 테스트를 그대로 통과한다.

## 7. B3 — 장기 실행 캐시의 크기 제한

### 현재 문제와 근거

`src/stats-compat.js::createStatsLoader()`의 Map은 TTL로 재사용만 제한한다. 동적으로 늘어나는 `derived:<기간>:<시작일>` 및 세션 참조 목록 기반 key는 TTL 만료만으로 삭제되지 않는다. 삭제 경로는 주로 `quotaRecovery`에 한정돼 있다.

따라서 장기 실행 시 오래된 기간/세션 집합의 결과가 남을 수 있다. 코드상 보존 경로를 확인한 것이며 사용자의 실제 메모리 증가량을 측정한 것은 아니다.

### 구현과 수용 기준

- 고정 자원과 동적 자원의 cache를 구분하고, 동적 cache에는 TTL 삭제와 명시적 최대 개수를 적용한다. 시작값 예시는 64개이며 이는 측정 기반 최적값이 아니라 조정 가능한 제안이다.
- 접근 시/삽입 시 만료 항목 정리 또는 bounded LRU를 구현한다. 진행 중 요청의 소유권은 저장 결과 cache와 분리해 A2/A3의 최신 요청 보호를 유지한다.
- 긴 세션 참조 목록 전체를 매번 key로 만들기보다 B4와 함께 session 단위 또는 제한된 batch 단위 key를 사용한다.
- 서로 다른 날짜/세션 집합 1,000회를 합성 실행해 cache 크기가 설정 상한을 넘지 않는지 검사한다. 테스트용 관찰 수단을 public facade 기능으로 노출할 필요는 없다.
- cache eviction 도중 pending/forced 요청이 완료돼도 만료된 세대가 결과를 되살리지 않아야 한다.
- 동일 요청 병합과 정상 TTL cache hit는 유지한다. 실제 provider 호출 증가 여부도 회귀 검증한다.

## 8. B4 — 대규모 세션 메타데이터 조회

### 현재 문제와 근거

`getStats({ includeSessionMetadata: true })`는 모든 기간의 session 참조를 합쳐 한 번에 요청한다. Rust `session_metadata.rs::collect()`는 `MAX_SESSION_REFS = 5_000`을 초과하면 거절한다. 프런트엔드는 이 오류를 무시하므로 참조가 5,000개를 넘는 경우 제목 보강 전체가 생략될 수 있다.

이 제한 자체는 필요한 방어 장치다. 제한을 무작정 높이거나 없애면 안 된다. 사용자의 현재 세션 수가 실제로 제한을 넘었다고 확인한 것은 아니다.

### 구현 순서

1. 현재 선택 기간과 화면에 필요한 session 참조를 우선한다. Today를 볼 때 All Time의 제목까지 필수 조회하지 않는다.
2. 참조를 중복 제거하고 5,000개 미만의 bounded batch로 요청한다. 초기 제안 batch 250~500개, 동시 처리 1~2개는 실제 I/O 측정 후 확정한다.
3. batch 단위 오류를 격리한다. 성공한 batch는 반영하고 실패한 batch만 basename/id fallback을 유지한다.
4. 승인된 provider title/project basename만 session 단위로 cache한다. transcript-derived fallback은 도입하지 않는다.
5. Rust의 입력 개수·session id 안전성 검증을 유지하고 UI 스크롤/선택 위치가 메타데이터 도착마다 흔들리지 않게 한다.

### 수용 기준

- 5,000 / 5,001 / 10,001개 참조에서도 개별 backend 요청은 상한 이하고 성공한 제목이 전체 소실되지 않는다.
- 일부 batch 오류, 중복 참조, 빈 ID, 빠른 기간 전환, 새 세션 추가가 서로 다른 세대의 metadata를 오염시키지 않는다.
- prompt/response/first-user-message/path 원문 sentinel이 renderer 계약에 포함되지 않는다.

## 9. B5 — 통합·실기기 검증 게이트

현재 pure-model, Rust, UI 구조 검사, 보안 설정, 패키징 테스트는 유효하다. 다만 소스 문자열/구조 검사는 실제 OS 창 동작을 증명하지 못하므로 아래를 별도 게이트로 유지한다.

- **제어 가능한 UI 통합 테스트:** 가짜 backend를 연결해 delayed bootstrap, 부분 실패, 강제 갱신 중 이전 결과 도착, null quota, 필터 dropdown overlay와 scroll 위치를 실제 DOM 동작으로 검증한다.
- **Windows 창:** taskbar 위 bubble 유지, auto-hide taskbar, 지원하는 taskbar 배치, 100/125/150/200% DPI, mixed-DPI 이동, monitor 분리, hover 확장/복귀, tray 복귀, Acrylic 비활성/복원.
- **드래그:** 기존 pointer-driven 경로 그대로 유지. 연속 드래그 중 snap/jump/offset, z-order keeper와 충돌, 렌더 갱신 hitch를 실기기로 확인한다. Port Lens 드래그 코드 이식으로 해결하지 않는다.
- **인증 복구:** 실제 Windows 계정에서 Claude/Gemini provider-owned credential 변경과 quota 회복 확인. 동일 거부 credential 재조회 금지, 429/Retry-After 유지, CLI process-tree 종료를 확인한다.
- **제공사:** 실제 값을 노출하는 Codex Business 계정의 `individualLimit`, 실행 중인 AGY의 quota. 가짜 fixture 통과를 live PASS로 기록하지 않는다.
- **패키지:** Windows CI의 check/Clippy/native build 및 installer/portable workflow 결과를 별도로 확인한다. macOS 빌드 통과를 Windows 런타임 검증으로 대체하지 않는다.

이 항목들은 이번에 새로 발생한 결함 목록이 아니라 기존 제품 목적을 보장할 검증 범위다. 비용·민감 데이터가 있는 live 검증은 소유자가 지정한 환경에서 진행한다.

### 후속 구현의 현재 수용 상태

| 항목 | 구현 / 검증 |
|---|---|
| B1 | 자원별 상태와 부분 patch, quota 독립 실행, 순차 usage 스캔, 실패한 기본 quota에서 독립 adapter 병합. Codex 누락 row를 unavailable로 초기화하고 확인된 workspace의 OAuth만 허용; 식별되지 않은 base의 App Server 보강은 생략. |
| B2 | status/recovery enum과 실제 제공사별 성공·시도·재시도 시각. diagnostic 문구와 동작 분리. credential 변경 시 last-good 폐기, stale 만료·reset 필터 보존. |
| B3 | derived 결과 64개 TTL/LRU 제한; 진행 요청의 소유권 분리. 서로 다른 1,000개 key 및 지연·강제 요청/eviction 회귀 검증. |
| B4 | 선택 기간 참조만 중복 제거, 250개씩 동시성 1로 수집. 4,096개/60초 session 단위 cache. 5,000/5,001/10,001개 참조, 부분 batch 실패, 새 세션, 기간 전환과 개인정보 sentinel 검증. 실 I/O 최적값을 주장하지 않는 보수적 초기값. |
| B5 자동화 | 가짜 Tauri backend를 주입한 실제 Chromium DOM 테스트 9개. bootstrap 지연/실패, 부분 갱신, 강제 갱신과 오래된 결과, null quota, rolling 날짜 변경, history 요청 세대, filter overlay/scroll 및 metadata 도착 검증. macOS/Windows CI에 추가. |
| B5 실환경 | Windows taskbar/DPI/drag/tray/Acrylic, 실제 Claude/Gemini 복구, Codex Business/AGY, 설치 패키지 시각 검증은 미실행. 지정 환경이 필요하며 fixture 통과로 대체하지 않음. |

후속 구현 로컬 결과: 프런트엔드 97 PASS, Rust 138 PASS/9 live ignored, DOM 9 PASS, production frontend build/rustfmt/Clippy/macOS native debug build PASS, npm audit 취약점 0. Windows CI와 installer/portable 결과는 해당 revision의 GitHub Actions에서 별도 확인한다.

B1 합성 실패를 수정 전 재현했고, B2의 문구 독립성 및 B3/B4의 크기·batch 테스트는 각각 이전 구현(`f84858f`, `8f9fb3b`)에서 실패하는 것을 확인했다. DOM 검증 중 날짜가 바뀐 rolling range의 실패가 이전 날짜 값을 남기는 회귀도 재현·수정했다. Today 미수집이 history/tray의 실제 값을 가짜 0으로 바꾸지 않으며, 강제 history 갱신도 오래된 응답을 차단한다.

재실행: 기존 `npm run check`에 더해 `npm exec playwright install chromium`, `npm run test:ui`. 브라우저 테스트는 실제 credential이나 제공사 API를 사용하지 않는다.

## 10. 최초 A1–A4 검수 당시 검증 결과

| 항목 | 변경 전 | 변경 후 / 이번 실행 |
|---|---|---|
| 프런트엔드 테스트 | 78 PASS | 84 PASS, 0 FAIL |
| Rust 테스트 | 129 PASS, 9 ignored | 132 PASS, 0 FAIL, 9 ignored (macOS) |
| 신규 결함 재현 | 없음 | 수정 전 JS 5개 + recovery 1개 + Rust 3개 FAIL 확인 |
| 프런트엔드 production build | PASS | PASS |
| rustfmt | PASS | PASS |
| Clippy `--all-targets -- -D warnings` | 이번 baseline에서 별도 재실행하지 않음 | PASS |
| macOS Tauri debug `--no-bundle` | 이번 baseline에서 별도 재실행하지 않음 | PASS |
| `npm audit --audit-level=high` | 이번 baseline에서 별도 재실행하지 않음 | 취약점 0건 |
| `git diff --check` | 작업트리 clean | PASS |
| Windows 실기기 / 실제 제공사 | 기존 STATE 이력만 참고 | 이번 작업에서 미실행 |

`ignored` 9개는 live 검증으로 남겨둔 테스트이며 통과로 집계하지 않았다. Unix 전용 stderr 테스트 1개 때문에 Windows Rust 실행 개수는 macOS와 다를 수 있다. 위 결과는 커밋된 코드 변경에 대한 로컬 확인이며, 이후 원격 CI 결과와 구분한다.

### 재실행 명령

```sh
npm ci
npm run check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm exec tauri build -- --debug --no-bundle
npm audit --audit-level=high
```

위 명령은 실제 provider 계정 테스트를 요구하지 않는다. `npm run test:live`는 별도 허가·환경 확인 없이 실행하지 않는다. 운영 credential이나 대화 원문을 CI fixture, 커밋, audit 첨부 파일로 만들지 않는다.

## 11. 구현·인수인계 규칙

1. 시작 시 `STATE.md`와 이 문서의 B1부터 읽고 최신 main/작업트리 상태를 다시 확인한다. 이 보고서의 baseline을 현재 HEAD로 오해하지 않는다.
2. B1 → B2 → B3/B4 순으로 작게 구현한다. B3/B4는 연계하되, quota 스케줄 변경과 한 커밋에 묶지 않는다.
3. 각 변경에 실패 재현 → 수정 → 회귀 통과 근거를 남긴다. B5의 필요한 실기기 게이트는 별도 기록한다.
4. 정상 quota는 tokScale 우선이고 보강은 누락 lane만 채운다. 다른 제공사·계정의 결과를 대체 입력으로 쓰지 않는다.
5. 기능 범위를 넓히거나 UI 재설계로 drift하지 않는다. 모니터의 정확성과 사용 편의가 개선되지 않는 추상화는 추가하지 않는다.
6. 이번 수정 rollback은 명시적 commit revert로 수행한다. reset/강제 push나 사용자 변경 삭제를 사용하지 않는다. 회귀 테스트는 가능하면 유지해 원인을 재현한다.
7. STATE는 현재 상태와 next action만 갱신한다. 완료 이력은 Git을 사용하고 이 보고서를 계속 늘리는 일지로 만들지 않는다.

## 12. 근거 탐색 지도

- 제품 목적·보존 제약: [`../AGENTS.md`](../AGENTS.md), [`architecture/v2-architecture.md`](architecture/v2-architecture.md), [`../README.md`](../README.md).
- A1: `renderer-model.js::optionalFinite/windowPercent/quotaRows`, `stats-compat.js::compatibilityWindow`, 추가 `renderer-model.test.js` 테스트.
- A2/A3: `stats-compat.js::createStatsLoader/cached/loadQuotaReport`, 추가 `stats-compat.test.js` 테스트.
- A4: `src-tauri/src/tokscale.rs::run/parse_json`와 같은 파일의 신규 3개 테스트.
- B1/B2: `src/main.js::refresh/mergeStatsPatch`, `src/stats-compat.js::getStats/quotaReportToCompatLimits`, `src-tauri/src/commands.rs::get_quota_report/get_quota_recovery_report`, `codex_business.rs::enrich_quota_report`.
- B3/B4: `src/stats-compat.js::sessionMetadataRefs/getStats/cached`, `src-tauri/src/session_metadata.rs::MAX_SESSION_REFS/collect`.
- B5: `STATE.md`의 Known open items, `.github/workflows/v2-ci.yml`, `.github/workflows/v2-build-windows.yml`, 기존 window/bubble 및 UI parity 테스트.
- 검수 baseline: [4bf1e42](https://github.com/teeeeooo/token-lens/tree/4bf1e428483c5d3b055029d654e7fc80df6ff272).
- 코드 수정: [fb861e7](https://github.com/teeeeooo/token-lens/commit/fb861e7d8e06a0900327d691701fc484239fadb1).
- 플랫폼 보안 참고: [Tauri CSP](https://v2.tauri.app/security/csp/), [Tauri capabilities](https://v2.tauri.app/security/capabilities/). CSP/capability가 정상 payload와 오류 문자열의 개인정보 최소화를 대신하지는 않는다.
