# Design Knowledge Engine — Design Spec

> ODE에 디자인 지식 엔진을 추가하여, AI 에이전트가 현업 수준의 디자인 판단을 내릴 수 있게 한다.

## 배경

### 현재 상태
- ODE 포맷, 렌더링 파이프라인, CLI, Figma 임포트 모두 완성
- 에이전트가 `.ode.json`을 읽고, 생성하고, 렌더링할 수 있음
- **부족한 것:** 에이전트가 "잘" 만들 수 있는 디자인 지식이 없음

### 동기
- ODE의 비전은 디자인 관련 모든 포맷의 표준화
- 에이전트가 포맷을 "읽을" 수 있지만, "좋은 디자인을 만들" 수 없음
- 현업 디자이너의 업무상 갖춰야 할 개념과 구조적 지식을 모두 호환해야 함

### 핵심 통찰: 지식은 생성과 검증 양쪽에 쓰인다
- **생성 시:** "랜딩페이지 만들어줘" → 디자인 지식을 참고해서 `.ode.json` 생성
- **리뷰 시:** 이미 만든 디자인을 규칙 기반으로 검증하고 피드백

---

## 섹션 1: 전체 아키텍처

### 2계층 구조

지식을 2가지 형태로 나눠서, 각 유형에 맞는 역할을 부여한다.

```
design-knowledge/
  rules/       ← 기계 검증 규칙 (JSON) — "이건 틀렸다"를 판단
  guides/      ← 에이전트용 가이드 (Markdown) — "이렇게 만들어야 한다"를 안내
  index.json   ← 전체 인덱스 (레이어 → 규칙/가이드 매핑)
```

별도 템플릿/예제 디렉토리 없음. 디자인 입력은 사용자에게서 온다 (설명, Figma 파일, 참고 이미지 등).

### 프로젝트 내 위치

```
open-design-engine/
  crates/
    ode-cli/       ← guide, review 커맨드 추가
    ode-core/
    ode-export/
    ode-format/
    ode-import/
    ode-review/    ← 새 크레이트: 디자인 규칙 검증 엔진
    ode-text/
  design-knowledge/
    rules/
    guides/
    index.json
```

`design-knowledge/`는 Rust 크레이트가 아니라 **데이터 패키지**. 컴파일에 포함되지 않고 `ode-cli`가 런타임에 참조.

### 에이전트 워크플로우

```
사용자 입력 (설명 / Figma / 참고이미지)
    │
    ├─ ode guide --context web  → 관련 디자인 지식 습득
    │
    ├─ 에이전트가 .ode.json 생성
    │
    ├─ ode build → 렌더링
    │
    └─ ode review → 규칙 기반 검증 → 수정 반복
```

### index.json 구조

규칙/가이드 파일 경로는 명시적 목록 (glob 패턴 미사용 — 의존성 최소화).

```json
{
  "layers": [
    {
      "id": "spatial-composition",
      "name": "공간 구성",
      "guides": ["guides/spatial-composition.md"],
      "rules": [
        "rules/spatial-composition/minimum-spacing.json",
        "rules/spatial-composition/alignment-consistency.json",
        "rules/spatial-composition/density-range.json"
      ]
    },
    {
      "id": "accessibility",
      "name": "접근성",
      "guides": ["guides/accessibility.md"],
      "rules": [
        "rules/accessibility/contrast-ratio.json",
        "rules/accessibility/touch-target-size.json",
        "rules/accessibility/focus-indicator.json",
        "rules/accessibility/cognitive-load.json"
      ]
    }
  ]
}
```

### 지식 탐색 경로 (Knowledge Discovery)

`ode-cli`가 `design-knowledge/` 경로를 찾는 방식:
1. 환경변수 `ODE_KNOWLEDGE_PATH` (우선)
2. 빌드 시 내장 경로 (`env!("CARGO_MANIFEST_DIR")` 기반 — `cargo install` 시에도 동작)
3. 실행 바이너리 기준 상대경로 `../design-knowledge/`
4. 현재 작업 디렉토리의 `design-knowledge/`
5. `~/.ode/design-knowledge/` (사용자 홈 디렉토리)

어느 경로에서도 찾지 못하면 `ode guide`/`ode review` 실행 시 명확한 에러 메시지 출력:
```json
{"status": "error", "code": "KNOWLEDGE_NOT_FOUND", "message": "design-knowledge directory not found", "suggestion": "set ODE_KNOWLEDGE_PATH or place design-knowledge/ in the working directory"}
```

---

## 섹션 2: Rules 계층

### 디렉토리 구조

```
design-knowledge/rules/
  accessibility/
    contrast-ratio.json
    touch-target-size.json
    focus-indicator.json
    cognitive-load.json
  spatial-composition/
    minimum-spacing.json
    alignment-consistency.json
    density-range.json
  typography/
    font-size-minimum.json
    line-height-ratio.json
    measure-length.json
  production/
    print-resolution.json
    bleed-margin.json
    color-space-match.json
  responsive/
    breakpoint-consistency.json
  platform/
    ios-safe-area.json
    material-elevation.json
    social-media-dimensions.json
```

### 규칙 스키마

규칙은 "무엇을 검사하는가"만 선언한다. 실제 검사 로직은 Rust의 네임드 체커 함수.

```json
{
  "id": "a11y-contrast-ratio-aa",
  "layer": "accessibility",
  "severity": "error",
  "checker": "contrast_ratio",
  "params": {
    "min_ratio": 4.5,
    "target": "text"
  },
  "applies_to": {
    "node_kinds": ["text"],
    "contexts": ["web", "mobile-app"]
  },
  "_note_node_kinds": "유효한 값: frame, group, vector, boolean-op, text, image, instance (wire format tag와 동일)",
  "message": "텍스트 대비 비율 {actual}:1 — WCAG AA 기준 {min_ratio}:1 이상 필요",
  "suggestion": "텍스트 색상을 더 어둡게 하거나 배경을 더 밝게 변경",
  "references": [
    "https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html"
  ]
}
```

### 필드 설명

| 필드 | 역할 |
|------|------|
| `severity` | `error` (반드시 수정), `warning` (권장), `info` (참고) |
| `checker` | Rust에 등록된 체커 함수 이름 |
| `params` | 체커 함수에 넘기는 파라미터 |
| `applies_to.node_kinds` | 노드 타입 필터. 유효값: `frame`, `group`, `vector`, `boolean-op`, `text`, `image`, `instance` (wire format tag와 동일) |
| `applies_to.contexts` | 출력 맥락 필터 — 웹 규칙이 인쇄에 적용되면 안 됨 |
| `message` | 에이전트용 에러 메시지 (템플릿 변수 `{actual}`, `{min_ratio}` 등 지원) |
| `suggestion` | 자동 수정 힌트 |
| `references` | 근거 URL |

### 체커 함수 목록

JSON은 설정, Rust는 로직 — 역할 분리.

| checker | 검사 내용 | params 예시 | 제공하는 템플릿 변수 |
|---------|----------|-------------|---------------------|
| `contrast_ratio` | 전경/배경 색상 대비 비율 | `min_ratio`, `target` | `{actual}`, `{min_ratio}`, `{fg_color}`, `{bg_color}` |
| `min_value` | 속성의 최소값 | `property`, `min`, `unit` | `{actual}`, `{min}`, `{property}` |
| `max_value` | 속성의 최대값 | `property`, `max`, `unit` | `{actual}`, `{max}`, `{property}` |
| `range` | 속성이 범위 안에 있는지 | `property`, `min`, `max` | `{actual}`, `{min}`, `{max}` |
| `ratio` | 두 값의 비율 | `numerator`, `denominator`, `min`, `max` | `{actual}`, `{min}`, `{max}` |
| `match` | 속성이 허용 값 중 하나인지 | `property`, `allowed` | `{actual}`, `{allowed}` |
| `spacing_scale` | 간격이 정해진 스케일을 따르는지 | `base`, `tolerance` | `{actual}`, `{base}`, `{nearest}` |
| `hierarchy` | 부모-자식 간 속성 비교 | `property`, `relation` | `{parent_value}`, `{child_value}` |

새 규칙 추가 시: 기존 checker + 다른 params면 JSON만 추가. 새 checker가 필요하면 Rust 추가.

### 메시지 템플릿 문법

`message`와 `suggestion` 필드는 `{key}` 형식의 템플릿 변수를 지원한다.

- 문법: `{variable_name}` — 단순 문자열 치환
- 사용 가능한 변수: 각 checker가 제공 (위 테이블의 "제공하는 템플릿 변수" 컬럼 참고)
- `params`의 키도 변수로 사용 가능 (예: `{min_ratio}`)
- 해석 불가능한 변수는 원문 그대로 유지 (에러 아님)

### contrast_ratio 체커: 배경색 결정 방식

텍스트의 전경색은 해당 노드의 fill에서 가져온다. 배경색은 다음 전략으로 결정:

1. 조상 노드를 위로 순회하며, **가장 가까운 solid fill을 가진 노드**의 첫 번째 solid fill 색상을 배경으로 사용
2. 조상 중 solid fill이 없으면 흰색(`#FFFFFF`)을 기본 배경으로 가정

**알려진 제한사항**: 그라데이션 배경, 이미지 배경, 여러 레이어의 opacity 합성은 무시된다. 이는 대부분의 실무 케이스를 커버하는 실용적 단순화이며, 향후 개선 가능.

### 규칙 로드 시 검증

`ode review` 실행 시, 규칙 파일을 로드하면서 다음을 검증:
- `checker` 이름이 등록된 Rust 체커와 일치하는지
- `params`에 해당 checker의 필수 키가 있는지

미등록 checker를 참조하는 규칙은 **건너뛰되 warning으로 보고** — 나머지 규칙은 정상 실행된다.

### 맥락(context) 결정 방식

1. `--context web` 플래그로 명시
2. 미지정 시 문서의 `views`에서 추론 (Print view → print, Web view → web)
3. **복수 view 타입이 존재하면**: 감지된 모든 context에 대해 규칙을 합산 실행 (union). 출력에 `"contexts_detected": ["web", "print"]` 포함.
4. views도 없으면 `web` 기본값

---

## 섹션 3: Guides 계층

### 디렉토리 구조

```
design-knowledge/guides/
  design-system.md
  project-structure.md
  design-principles.md
  platform-conventions/
    web.md
    ios.md
    android.md
    print.md
    presentation.md
    social-media.md
  spatial-composition.md
  responsive-adaptive.md
  content-strategy.md
  accessibility.md
  motion-interaction.md
```

### 가이드 문서 포맷

모든 가이드가 동일한 구조를 따른다. 에이전트가 예측 가능하게 파싱할 수 있도록.

```markdown
---
id: spatial-composition
name: 공간 구성
layer: spatial-composition
contexts: [web, mobile-app, print, presentation]
related: [design-principles, responsive-adaptive, accessibility]
---

# 공간 구성 (Spatial Composition)

## 핵심 원칙
[이 레이어의 3~5가지 핵심 원칙. 간결하게.]

## 규칙
[구체적, 적용 가능한 규칙들. 수치 포함.]

## 맥락별 적용
[context별로 다르게 적용되는 부분]

## ODE 매핑
[이 지식을 ODE 포맷으로 어떻게 표현하는지. 인라인 JSON 스니펫 포함.]

## 안티패턴
[에이전트가 하지 말아야 할 것들]
```

### 설계 원칙

1. **YAML frontmatter**: `related`, `contexts` — 에이전트가 어떤 가이드를 읽을지 빠르게 판단
2. **"ODE 매핑" 섹션이 핵심**: 추상 원칙이 아니라, ODE의 어떤 필드에 어떤 값을 넣으면 되는지 구체적
3. **인라인 JSON 스니펫**: 별도 예제 파일 대신, 가이드 안에서 원칙 설명 시 작은 ODE JSON 조각 포함
4. **안티패턴**: 에이전트가 가장 자주 하는 실수를 명시

---

## 섹션 4: 9개 레이어 콘텐츠 범위

### 1. 디자인 시스템 (design-system.md)

- 토큰 체계: primitive → semantic → component 네이밍 (`{category}.{property}.{variant}.{state}`)
- 컬러 팔레트 구성: primary/secondary/neutral/semantic(error, success, warning)
- 타이포그래피 스케일: major third(1.25), perfect fourth(1.333) 비율 체계
- 스페이싱 스케일: 4px 또는 8px 기반
- 컴포넌트 토큰화: `button.bg` → `color.primary` (간접 참조)
- **컴포넌트 해부학**: 버튼(container+label+icon), 카드(container+media+content+actions), 입력(container+label+field+helper) 등 공통 컴포넌트의 노드 구성
- **상태/변형 모델**: default/hover/pressed/disabled/focus 상태, primary/secondary/ghost/danger 변형. ODE의 `ComponentDef` + `Instance` + `overrides`로 표현
- ODE 매핑: `DesignTokens`, `TokenCollection`, `StyleValue::Bound`, `ComponentDef`, `InstanceData`

### 2. 프로젝트 구조 (project-structure.md)

- 캔버스 조직: 페이지별 루트 프레임, 네이밍 관례
- 노드 네이밍 규칙: 케밥케이스, 역할 기반 (`hero-section`, `cta-button`)
- 계층 깊이 관리: 최대 5~6 레벨 권장
- 컴포넌트 정의 위치: 별도 "Components" 캔버스 루트 또는 문서 분리
- 뷰 활용: 같은 캔버스에서 Print/Web/Export 뷰 분리 관리
- ODE 매핑: `Document.canvas`, `Node.name`, `ComponentDef`, `View`

### 3. 디자인 원칙 (design-principles.md)

- 색상 이론: 보색, 유사색, 삼각배색, 60-30-10 비율 규칙
- 타이포그래피: 서체 조합 (serif + sans-serif), 위계 표현 (size, weight, color)
- 시각적 무게: 크기, 색상 채도, 대비가 시선을 끄는 순서
- 일관성: 같은 역할 → 같은 스타일
- ODE 매핑: `Fill`, `Stroke`, `TextStyle`, `Effect`

### 4. 플랫폼 규약 (platform-conventions/)

**web.md**
- 브레이크포인트: 320, 768, 1024, 1440
- 네비게이션 패턴, 버튼 크기 (최소 40px), 그리드 (12컬럼, gutter 24px)
- 출력 규격: sRGB, 72/96 DPI, PNG/SVG/PDF 선택 기준

**ios.md**
- Safe Area, Dynamic Type, SF Symbols
- 터치 타겟 44×44pt, 탭바 최대 5개
- 출력 규격: Display P3, @1x/@2x/@3x

**android.md**
- Material Design 3: 컬러 롤, 다이나믹 컬러
- 48×48dp 터치 타겟, bottom navigation, navigation rail
- 출력 규격: sRGB, dp 단위

**print.md**
- 표준 규격: A4, A3, 명함(91×55mm), Letter
- 재단선 3mm, 안전영역 5mm
- 출력 규격: CMYK, 300DPI 최소, ICC 프로파일

**presentation.md**
- 16:9 (1920×1080) 표준
- 한 슬라이드 한 메시지, 폰트 제목 36pt+, 본문 24pt+
- 여백 최소 5%

**social-media.md**
- Instagram (1080×1080, 1080×1350), YouTube 썸네일 (1280×720), OG 이미지 (1200×630)
- 텍스트 안전 영역, 각 플랫폼별 규격
- 출력 규격: sRGB, PNG/JPEG

### 5. 공간 구성 (spatial-composition.md)

- 게슈탈트 원칙: 근접성, 유사성, 폐합, 연속성
- 여백 체계: 관계 밀접도에 비례 — 같은 그룹 8~12px, 섹션 간 48~80px
- 정렬: 보이지 않는 선 — 요소의 시작점/끝점 일치
- 시각적 밸런스: 대칭 vs 비대칭
- **시각적 리듬**: 간격의 반복 패턴이 섹션 구분감을 만듦
- **밀도 수준**: compact(대시보드) / comfortable(기본) / spacious(마케팅) — 맥락에 따라 조절
- 황금비, 3분할법
- ODE 매핑: `LayoutConfig`, `padding`, `item_spacing`, `transform`

### 6. 반응형/적응형 (responsive-adaptive.md)

- 브레이크포인트 전략: mobile-first vs desktop-first
- 유동적 타이포그래피: 뷰포트에 따라 font-size 스케일링
- 콘텐츠 재배치: 2단→1단, 사이드바→하단 접힘
- 프로그레시브 디스클로저: 작은 화면에서 정보 축소
- 터치 vs 마우스: 터치 환경에서 더 큰 타겟, 더 넓은 간격
- ODE 매핑: `Constraints`, `--resize`, `View::Web { breakpoints }`

### 7. 콘텐츠 전략 (content-strategy.md)

- 정보 위계: 제목→부제→본문→보조 (크기/무게/색상으로 구분)
- 읽기 패턴: F-패턴 (텍스트), Z-패턴 (비주얼)
- 페이지 구조 관례: 랜딩(히어로→가치제안→기능→소셜프루프→CTA→푸터), 대시보드(요약 상단→상세 하단)
- 카피 길이: 헤드라인 5~8단어, 서브헤드 15~25단어
- CTA 문구: 동사 시작
- **빈/에러/로딩 상태**: 데이터 없을 때, 실패 시, 로딩 중 표시 가이드
- **아이콘 사용**: 아이콘만 / 텍스트만 / 아이콘+텍스트 판단 기준, 크기 관계 (font-size × 1.25~1.5)
- **데이터 시각화 기초**: 차트 선택 기준, 숫자 강조, 테이블 레이아웃
- ODE 매핑: node hierarchy, `TextData.content`, 노드 순서

### 8. 접근성 (accessibility.md)

- WCAG AA: 일반 텍스트 4.5:1, 대형 텍스트 3:1 대비
- 터치/클릭 타겟: 44×44pt (iOS), 48×48dp (Android)
- 색상만으로 정보 전달 금지
- 포커스 표시자: 2px 이상, 배경과 3:1 대비
- 텍스트 리사이징: 200%까지 깨지지 않는 레이아웃
- 모션: prefers-reduced-motion 대응
- 국제화: RTL 미러링, CJK 줄바꿈, 번역 시 150% 텍스트 확장 대비
- **인지 접근성**: 명확한 언어, 예측 가능한 네비게이션, 정보 과부하 방지, 단계적 공개
- ODE 매핑: `Color` 대비 계산, `LayoutConfig` 최소 크기, `TextStyle`

### 9. 모션/인터랙션 (motion-interaction.md)

범위 축소 — 토큰 선언 가이드 수준.

- 지속 시간 관례: 마이크로 100~200ms, 트랜지션 200~500ms
- 이징: ease-out (진입), ease-in (퇴장), ease-in-out (상태 변경)
- 의미론: 들어옴/나감/강조/피드백
- 접근성: prefers-reduced-motion 시 대응
- ODE 매핑: `TokenValue::Duration`, `TokenValue::CubicBezier`

---

## 섹션 5: CLI 확장

### 새 커맨드

기존 CLI에 2개 추가: `ode guide`, `ode review`.

#### `ode guide`

디자인 지식 조회.

```bash
ode guide                              # 전체 레이어 목록 (JSON)
ode guide spatial-composition          # 특정 가이드 전문 (마크다운)
ode guide spatial-composition --section rules  # 특정 섹션만
ode guide --context print              # 인쇄 관련 가이드 필터 (JSON)
ode guide --related accessibility      # 관련 가이드 나열 (JSON)
```

출력 규칙 — 모든 출력은 JSON envelope로 감싼다 (기존 CLI의 "모든 출력은 JSON" 원칙 준수):

가이드 내용 조회:
```json
{"status": "ok", "format": "markdown", "content": "# 공간 구성\n\n## 핵심 원칙\n..."}
```

목록 조회:
```json
{"status": "ok", "layers": [{"id": "spatial-composition", "name": "공간 구성", ...}]}
```

#### `ode review`

디자인 규칙 기반 검증. 기존 `ode validate`(구조 검증)와 별도.

```bash
ode review design.ode.json                    # context 자동 추론
ode review design.ode.json --context web      # 웹 규칙으로 검증
ode review design.ode.json --layer accessibility  # 특정 레이어만
```

### validate vs review

| | `ode validate` | `ode review` |
|--|----------------|--------------|
| 검사 대상 | 문서 구조 | 디자인 품질 |
| 실패 시 | 렌더링 불가 | 렌더링은 됨, 품질 문제 |
| 규칙 출처 | ode-cli 내장 | `design-knowledge/rules/` |
| 포함 관계 | `ode build`에 포함 | 에이전트가 명시적으로 호출 |

### `ode review` 출력

기존 CLI 출력 규약과 일관: `"status": "ok"` (리뷰 완료), issues 배열은 기존 `ValidationIssue`와 호환되는 구조.

```json
{
  "status": "ok",
  "context": "web",
  "summary": {
    "errors": 1,
    "warnings": 2,
    "passed": 23,
    "total": 26
  },
  "issues": [
    {
      "severity": "error",
      "code": "a11y-contrast-ratio-aa",
      "layer": "accessibility",
      "path": "nodes[3]",
      "message": "텍스트 대비 비율 3.2:1 — WCAG AA 기준 4.5:1 이상 필요",
      "suggestion": "텍스트 색상을 더 어둡게 하거나 배경을 더 밝게 변경"
    }
  ]
}
```

**validate 출력과의 호환성:** `code`, `path`, `message`, `suggestion` 필드는 기존 `ode validate` 출력과 동일. `severity`와 `layer`가 추가 필드. 에이전트가 동일한 파싱 로직으로 양쪽 결과를 처리 가능.

### 종료 코드

기존 CLI 종료 코드 체계를 따른다:
- `0` — 리뷰 완료 (issues 유무와 무관 — "issues 있음"은 성공적 리뷰)
- `1` — 입력 에러 (JSON 파싱 실패, 파일 읽기 불가)
- `2` — I/O 에러 (출력 쓰기 실패, knowledge 파일 읽기 실패)
- `4` — 내부 에러 (예상치 못한 패닉)

### 새 크레이트: `ode-review`

```
ode-format  (data model)
    ^
ode-core    (rendering + layout)
    ^
ode-review  (design rules engine, depends on ode-format + ode-core)
    ^
ode-cli     (CLI, depends on all)
```

`ode-review`의 의존성:
- `ode-format` — 노드 트리 순회, 속성 접근 (`Document`, `Node`, `NodeKind`)
- `ode-core` — 색상 변환 로직 (`contrast_ratio` 계산에 필요)

`ode-review`는 **해석 완료된 `Document`** (StableId → NodeId 변환 후)에 대해 동작한다. wire format(`DocumentWire`)이 아님. 이유: checker들이 토큰 바인딩이 해석된 실제 색상값, 레이아웃 계산 결과 등에 접근해야 함.

`ode-review`가 별도 크레이트인 이유:
- contrast_ratio 계산에 색상 변환 로직 필요 (`ode-core` 의존)
- 규칙 파일 파싱 + checker 레지스트리 + 노드 트리 순회 — 독립적 책임
- 라이브러리로 재사용 가능

**참고:** 기존 구조 검증(`ode validate`)은 `ode-cli` 내장. 향후 `ode-review`가 입력 문서의 구조적 정합성을 사전 확인해야 할 경우, 검증 로직을 공유 위치(`ode-format` 또는 별도 `ode-validate`)로 추출하는 리팩터링을 고려.

---

## 섹션 6: 지식 수집 전략

### 출처 계층

**1차 출처 (공식 가이드라인):**

| 레이어 | 소스 |
|--------|------|
| 플랫폼 — Web | Google Web Fundamentals, MDN Web Docs |
| 플랫폼 — iOS | Apple Human Interface Guidelines (2025) |
| 플랫폼 — Android | Material Design 3 (material.io) |
| 플랫폼 — Print | ISO 216, Fogra/ICC |
| 접근성 | WCAG 2.2, WAI-ARIA Practices |
| 모션 | Material Motion, Apple Motion Principles |

**2차 출처 (업계 합의):**

| 레이어 | 소스 |
|--------|------|
| 디자인 시스템 | W3C Design Tokens Format, Figma Variables |
| 공간 구성 | 8-point grid (Spec.fm), Gestalt 심리학 |
| 타이포그래피 | Butterick's Practical Typography, Google Fonts Knowledge |
| 색상 이론 | OKLCH (CSS Color Level 4), Huetone |
| 반응형 | CSS Container Queries, Every Layout |

**3차 출처 (트렌드 & 실증):**

| 레이어 | 소스 |
|--------|------|
| 콘텐츠 전략 | Nielsen Norman Group, Baymard Institute |
| 소셜 미디어 | 각 플랫폼 공식 가이드 |
| 전체 트렌드 | Figma Config, Apple WWDC 디자인 세션 |

### 수집 원칙

1. **공식 > 관행 > 트렌드** — 충돌 시 공식 가이드라인 우선
2. **수치를 명시** — "적당한 여백"이 아니라 "16px 이상"
3. **근거 링크 포함** — 가이드에 출처 URL 명시
4. **버전 명시** — "WCAG 2.2 (2023)", "Material Design 3 (2024)"
5. **갱신 주기** — 연 1회. 플랫폼 규약은 WWDC/I·O 이후, 접근성은 WCAG 새 버전 발행 시

### 구현 시 수집 방법

- `context7` MCP로 라이브러리 문서 조회
- `WebFetch`/`WebSearch`로 최신 가이드라인 확인
- 1차 출처 원문을 읽고, ODE 컨텍스트에 맞게 재구성
- 원문 복사가 아닌 **원칙 추출 + ODE 매핑** 형태로 작성

---

## 구현 범위

### 변경/추가 크레이트

| 크레이트 | 변경 내용 |
|---------|----------|
| `ode-review` (새) | 규칙 파일 파싱, checker 레지스트리, 노드 트리 순회, 검증 결과 생성 |
| `ode-cli` (변경) | `guide`, `review` 서브커맨드 추가, knowledge path discovery |
| `design-knowledge/` (새) | 규칙 JSON + 가이드 마크다운 콘텐츠 |

### ode-cli 의존성 변화

```
기존: ode-cli → ode-format, ode-core, ode-export, ode-import, ode-text
추가: ode-cli → ode-review (새)
```

### 구현 단계

**Phase 1 — 엔진 + 핵심 2개 레이어** (end-to-end 검증)
- `ode-review` 크레이트: 규칙 로더, checker 레지스트리, 노드 순회
- `ode-cli`: `ode guide`, `ode review` 커맨드
- `design-knowledge/index.json` + discovery
- 콘텐츠: `accessibility` 레이어 (가이드 + 규칙 3~4개), `spatial-composition` 레이어 (가이드 + 규칙 3~4개)
- checker 구현: `contrast_ratio`, `min_value`, `spacing_scale`

**Phase 2 — 나머지 레이어 확장**
- 나머지 7개 레이어 가이드 작성
- 추가 checker 구현 (`max_value`, `range`, `match`, `hierarchy`, `ratio`)
- 플랫폼별 규칙 추가

### 테스트 전략

- **ode-review**: checker별 단위 테스트, 규칙 파일 파싱 테스트, 통합 검증 테스트
- **ode-cli**: `ode guide` 출력 테스트, `ode review` 통합 테스트
- **design-knowledge**: 모든 규칙 JSON이 스키마에 부합하는지 검증, 모든 가이드가 frontmatter 포맷을 따르는지 검증
