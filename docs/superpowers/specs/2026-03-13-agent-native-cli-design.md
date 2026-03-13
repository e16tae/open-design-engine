# Agent-Native CLI Design Spec

> ode-cli를 CLI-Anything 철학에 따라 AI 에이전트 최적화 인터페이스로 재설계하고, ode-mcp를 폐지한다.

## 배경

### 현재 상태
- `ode-cli`는 `render`와 `info` 두 개의 서브커맨드만 제공 (87줄)
- `ode-mcp`는 빈 스켈레톤 상태
- `.ode.json`의 children/canvas가 `NodeId`(slotmap 내부 키)로 직렬화되어 외부에서 문서 생성 불가

### 동기
- CLI-Anything 접근법: 소프트웨어를 AI 에이전트가 CLI로 제어할 수 있게 만드는 방식
- CLI는 보편적 인터페이스 — 모든 AI 에이전트(Claude Code, Cursor, OpenCode 등)가 사용 가능
- MCP는 특정 프로토콜에 종속되며, CLI 위에 래퍼로 나중에 추가 가능 (역은 불가)

### 핵심 통찰: 선언적 문서 우선 (Declarative Document-First)
`.ode.json` 포맷 자체가 API다. 에이전트는 JSON 문서를 생성하는 데 최적화되어 있으므로, 개별 서브커맨드를 수십 개 호출하는 것보다 전체 문서를 한 번에 작성하고 빌드하는 것이 근본적으로 빠르다.

| 기준 | 서브커맨드 방식 | 배치 연산 | **선언적 (채택)** |
|------|----------|---------|----------|
| 호출 횟수 | N개 | 1개 | **1개** |
| 에이전트 사고모델 | 절차적 순서 관리 | 연산 순서 + $ref 관리 | **결과물만 기술** |
| 멱등성 | 없음 | 없음 | **있음** |
| LLM 친화성 | 낮음 | 중간 | **최고** |
| 파일 I/O | N회 | 1회 | **1회** |

---

## 섹션 1: 포맷 변경 — StableId 기반 참조

### 문제
현재 `NodeId`(slotmap 내부 키)가 직렬화에 노출되는 위치:
- `ContainerProps.children: Vec<NodeId>`
- `GroupData.children: Vec<NodeId>`
- `BooleanOpData.children: Vec<NodeId>`
- `Document.canvas: Vec<NodeId>`
- `ViewKind::Print { pages: Vec<NodeId> }`
- `ViewKind::Web { root: NodeId }`
- `ViewKind::Presentation { slides: Vec<NodeId> }`

에이전트는 이 값들을 생성할 수 없다.

### 변경점

**1. NodeTree 직렬화 방식 변경**

현재: `SlotMap<NodeId, Node>`를 slotmap 기본 직렬화로 저장 (내부 키 노출).

변경: JSON에서는 `Vec<Node>` 형태로 직렬화. 각 노드는 `stable_id`로 식별.

**2. 모든 NodeId 참조 → StableId 직렬화**

대상 (직렬화 시 `StableId` 사용, 런타임은 `NodeId` 유지):
- `ContainerProps.children`
- `GroupData.children`
- `BooleanOpData.children`
- `Document.canvas`
- `ViewKind::Print.pages`
- `ViewKind::Web.root`
- `ViewKind::Presentation.slides`

**3. 역직렬화 2-pass 전략**

로드 시 다음 순서로 처리:
1. **Pass 1**: JSON의 `nodes` 배열을 `Vec<Node>`로 역직렬화. 각 Node를 SlotMap에 삽입하여 `NodeId` 할당. `stable_id → NodeId` 매핑 테이블 구축.
2. **Pass 2**: 매핑 테이블을 사용하여 모든 `StableId` 참조를 `NodeId`로 변환. 대상: children, canvas, views의 모든 NodeId 필드.

이 로직은 `Document` 수준의 커스텀 `Deserialize` 구현에 위치한다. `NodeTree` 단독이 아니라 `Document` 전체가 커스텀 역직렬화 대상이다 (canvas와 views가 NodeTree 외부에 있으므로).

**4. 포맷 버전**

이 변경은 직렬화 호환성을 깨뜨린다. `format_version`을 `0.2.0`으로 범프. 구버전(0.1.0) 파일은 지원하지 않는다 (프로젝트가 아직 v0.1.0 프리릴리즈이므로 하위 호환 부담 없음).

### 에이전트가 작성하는 .ode.json 예시

```json
{
  "format_version": [0, 2, 0],
  "name": "Landing Page",
  "working_color_space": "srgb",
  "nodes": [
    {
      "stable_id": "root",
      "name": "Page",
      "kind": {
        "type": "frame",
        "width": 1440,
        "height": 900,
        "visual": {
          "fills": [{
            "paint": {"type": "solid", "color": {"space": "srgb", "r": 1, "g": 1, "b": 1, "a": 1}},
            "opacity": 1.0,
            "blend_mode": "normal",
            "visible": true
          }]
        },
        "container": {"children": ["header", "hero"]}
      }
    },
    {
      "stable_id": "header",
      "name": "Header",
      "kind": {
        "type": "frame",
        "width": 1440,
        "height": 80,
        "visual": {},
        "container": {}
      }
    },
    {
      "stable_id": "hero",
      "name": "Hero Section",
      "kind": {
        "type": "frame",
        "width": 1440,
        "height": 600,
        "visual": {},
        "container": {}
      }
    }
  ],
  "canvas": ["root"],
  "tokens": {"collections": [], "active_modes": {}},
  "views": []
}
```

### 런타임 영향
- 내부 코드는 `NodeId` 계속 사용 — 성능 변화 없음
- 변경은 직렬화 레이어에만 한정 — `Serialize`/`Deserialize` 커스텀 구현
- 기존 테스트 — 내부 로직 테스트는 변경 없음, JSON 라운드트립 테스트만 업데이트

---

## 섹션 2: CLI 커맨드 상세 설계

### 공통 규칙
- **모든 출력은 JSON** — 사람용 텍스트 없음, 에이전트가 stdout을 파싱
- **에러도 JSON** — `{"status":"error", "code":"...", "message":"...", ...}`
- **종료 코드** — 성공 `0`, 입력 에러(파싱+검증) `1`, I/O 에러 `2`, 처리 에러(렌더+내보내기) `3`, 내부/예상치 못한 에러 `4`
- **stdin 지원** — 입력을 받는 커맨드(`validate`, `build`, `render`, `inspect`)에서 파일 경로 대신 `-`를 쓰면 stdin에서 읽음. `new`와 `schema`는 해당 없음.
- **stderr** — 프로세스 수준의 디버깅 메시지만 (에이전트는 무시)
- **`--help`/`--version`** — 예외적으로 사람용 텍스트 출력 (clap 기본 동작). 에이전트는 이 플래그를 사용할 필요 없음.

### 커맨드별 설계

#### `ode new <file>`

새 빈 문서를 생성하고 파일에 저장.

```bash
ode new design.ode.json
ode new design.ode.json --name "My App" --width 1440 --height 900
```

출력:
```json
{"status": "ok", "path": "design.ode.json"}
```

`--width`/`--height`를 주면 해당 크기의 루트 Frame을 canvas에 자동 추가.

#### `ode validate <file>`

문서를 파싱하고 구조적 오류를 검사. 렌더링 하지 않음.

```bash
ode validate design.ode.json
echo '...' | ode validate -
```

성공:
```json
{"valid": true, "warnings": []}
```

실패:
```json
{
  "valid": false,
  "errors": [
    {"path": "nodes[1].kind.container.children[0]", "code": "INVALID_REFERENCE", "message": "referenced stable_id 'xyz' not found", "suggestion": "available stable_ids: [\"header\", \"hero\"]"}
  ],
  "warnings": [
    {"path": "nodes[0].kind.visual.fills[0].paint.color", "code": "CMYK_FALLBACK", "message": "CMYK color will fall back to black in PNG export"}
  ]
}
```

검증 항목:
- JSON 스키마 적합성
- `stable_id` 중복 검사
- children/canvas/views의 `stable_id` 참조 유효성
- 순환 부모-자식 관계 검사
- 토큰 참조 유효성
- `InstanceData.source_component`가 `component_def`를 가진 노드를 가리키는지 검사

#### `ode build <file> -o <output>`

validate → convert → render → export를 한 번에 수행.

```bash
ode build design.ode.json -o output.png
echo '...' | ode build - -o output.png
```

성공:
```json
{"status": "ok", "path": "output.png", "width": 1440, "height": 900, "warnings": []}
```

실패 (검증 단계):
```json
{"status": "error", "phase": "validate", "errors": [...]}
```

실패 (렌더링 단계):
```json
{"status": "error", "phase": "render", "message": "..."}
```

#### `ode render <file> -o <output>`

빠른 렌더링. 구조적 파싱과 StableId → NodeId 해석은 수행하지만 (필수 — 없으면 렌더링 불가), 명시적 검증 리포트(중복 ID, 순환 검사 등)는 건너뛴다. 이미 `validate`를 통과한 문서에 대해 반복 렌더링할 때 유용.

```bash
ode render design.ode.json -o output.png
```

출력은 `build`와 동일한 형태.

#### `ode inspect <file>`

에이전트가 기존 문서의 구조를 파악할 때 사용.

```bash
ode inspect design.ode.json
```

```json
{
  "name": "Landing Page",
  "format_version": "0.1.0",
  "working_color_space": "srgb",
  "node_count": 12,
  "canvas": ["root"],
  "tree": [
    {
      "stable_id": "root",
      "name": "Page",
      "kind": "frame",
      "size": [1440, 900],
      "children": [
        {
          "stable_id": "header",
          "name": "Header",
          "kind": "frame",
          "size": [1440, 80],
          "children": []
        }
      ]
    }
  ],
  "tokens": {
    "collections": ["Colors", "Spacing"],
    "total_tokens": 24
  }
}
```

`--full` 플래그로 모든 속성(visual, transform 등)을 포함한 전체 덤프 가능.

#### `ode schema`

에이전트가 `.ode.json` 포맷을 학습하는 데 사용. JSON Schema 출력.

```bash
ode schema              # 전체 문서 스키마
ode schema node         # Node 타입만
ode schema paint        # Paint 타입만
ode schema token        # Token 시스템만
```

스키마는 코드에서 `schemars` 크레이트로 자동 생성.

---

## 섹션 3: 에러 처리

### 에러 코드 체계

| 코드 | 단계 | 의미 |
|------|------|------|
| `PARSE_FAILED` | parse | JSON 파싱 실패 |
| `SCHEMA_VIOLATION` | validate | 필수 필드 누락, 타입 불일치 |
| `INVALID_REFERENCE` | validate | stable_id 참조가 존재하지 않음 |
| `DUPLICATE_ID` | validate | stable_id 중복 |
| `CIRCULAR_HIERARCHY` | validate | 부모-자식 순환 |
| `CYCLIC_TOKEN` | validate | 토큰 별칭 순환 |
| `INVALID_COMPONENT_REF` | validate | Instance의 source_component가 유효하지 않음 |
| `RENDER_FAILED` | render | 렌더링 중 오류 |
| `EXPORT_FAILED` | export | 파일 쓰기 실패 |
| `IO_ERROR` | io | 파일 읽기/쓰기 실패 |

`suggestion` 필드는 에이전트가 자동 수정할 수 있는 힌트를 제공한다. 잘못된 stable_id 참조 시 사용 가능한 ID 목록, 스키마 위반 시 기대되는 타입 등.

### 에이전트 워크플로우

**생성:**
```
1. ode schema             ← 포맷 학습 (최초 1회)
2. ode new app.ode.json   ← 빈 문서 생성
3. (에이전트가 .ode.json 직접 작성)
4. ode build app.ode.json -o out.png  ← 빌드+렌더 (내부적으로 검증 포함)
   ├─ ok → 완료
   └─ error → 에이전트가 에러 읽고 JSON 수정 → 4 반복
```

**수정:**
```
1. ode inspect app.ode.json       ← 현재 구조 파악
2. ode inspect app.ode.json --full ← 상세 속성 필요 시
3. (에이전트가 .ode.json 파일 직접 수정)
4. ode build app.ode.json -o out.png
```

---

## 섹션 4: 구현 범위

### ode-mcp 폐지
- `crates/ode-mcp` 디렉토리 삭제
- `Cargo.toml` workspace members에서 제거

### 새 의존성

| 크레이트 | 용도 |
|---------|------|
| `schemars` | Rust 타입에서 JSON Schema 자동 생성 |

### schemars 호환성 주의사항

코드베이스의 `#[serde(untagged)]` 타입(`StyleValue<T>`, `TokenResolve`)은 schemars가 `anyOf` 스키마를 생성한다. 이는 기술적으로 정확하지만 LLM이 해석하기 어려울 수 있다. 대응:
- 생성된 스키마에 `description` 어노테이션을 수동으로 보강 (`schemars(description = "...")` attribute)
- `#[serde(tag = "type", content = "value")]` (adjacently tagged, `TokenValue`에 사용)의 schemars 호환성을 구현 시 검증
- `ode schema` 출력은 자동 생성 후 검수 과정을 거쳐 릴리즈

### 변경 크레이트별 범위

**ode-format (변경)**
- `NodeTree` 커스텀 `Serialize`/`Deserialize` — `Vec<Node>` 형태로 직렬화
- 모든 `NodeId` 참조 필드 — 직렬화 시 `StableId`, 로드 시 매핑 변환 (children, canvas, views)
- `Document` 수준 커스텀 `Deserialize` — 2-pass 전략으로 StableId → NodeId 해석
- 모든 주요 타입에 `schemars::JsonSchema` derive 추가 (untagged enum은 description 보강)
- 기존 JSON 라운드트립 테스트 업데이트

**ode-core (변경 없음)**
- 내부적으로 `NodeId` 계속 사용. 직렬화 레이어 변경의 영향 없음

**ode-export (변경 없음)**
- PNG 내보내기 로직 그대로

**ode-cli (대규모 재작성)**
- 현재 87줄 → 예상 ~600줄
- 6개 서브커맨드: `new`, `validate`, `build`, `render`, `inspect`, `schema`
- 공통 JSON 출력 모듈
- 구조화된 에러 핸들링
- stdin(`-`) 입력 지원
- 검증 엔진 (참조 유효성, 순환 검사, 스키마 적합성)

**ode-mcp (삭제)**

### 테스트 전략
- **ode-format**: StableId 기반 직렬화 라운드트립 테스트, 기존 41개 테스트 업데이트
- **ode-cli**: 각 커맨드별 통합 테스트 (유효 입력, 에러 케이스, stdin 입력)
- **검증 엔진**: 에러 코드별 단위 테스트 (INVALID_REFERENCE, DUPLICATE_ID, CIRCULAR_HIERARCHY 등)
