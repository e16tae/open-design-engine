---
id: agent-image-workflow
name: 이미지 워크플로우
layer: image-pipeline
contexts: [web, print, presentation]
related: [spatial-composition, accessibility]
---

# 에이전트 이미지 워크플로우 가이드

이 가이드는 AI 에이전트가 ODE 디자인 파일에 이미지를 삽입하고 렌더링하기 위한
실용 지침을 제공합니다. 이 문서 하나만으로 이미지 작업이 가능하도록 작성되었습니다.


## 이미지 라이프사이클 개요

ODE에서 이미지는 4단계를 거칩니다:

```
입력 (Input) → 저장 (Storage) → 렌더링 (Rendering) → 내보내기 (Export)
```

1. **입력:** `document.json`에 이미지 노드를 추가하고, 이미지 소스(경로 또는 바이트)를 지정
2. **저장:** AssetStore가 SHA-256 해시로 이미지를 관리 (팩드 .ode 파일 또는 언팩 디렉토리)
3. **렌더링:** `ode build`가 이미지 데이터를 로드하여 tiny-skia 픽스맵에 그림
4. **내보내기:** PNG 직접 출력, SVG는 base64 인라인, PDF는 krilla 임베딩

### Linked vs Embedded

| 방식 | 설명 | 사용 시점 |
|------|------|----------|
| **Linked** | 외부 파일 경로를 참조 (`"source": {"type": "linked", "path": "..."}`) | 로컬 작업, 빌드 시 파일이 존재하는 환경 |
| **Embedded** | 바이트 배열을 직접 포함 (`"source": {"type": "embedded", "data": [...]}`) | Figma 임포트 등 프로그래밍 방식 생성 시 |

**권장:** 에이전트 워크플로우에서는 **Linked** 방식을 사용합니다.
이미지 파일을 디스크에 생성하고 그 절대 경로를 `source.path`에 지정하면 됩니다.


## 워크플로우 레시피

### 레시피 A: document.json 직접 작성 (권장)

가장 유연하고 확실한 방법입니다. document.json에 이미지 노드를 직접 작성합니다.

**1단계: 이미지 파일 준비**

```bash
# 예: Python으로 100x100 빨간 PNG 생성
python3 -c "
from PIL import Image
img = Image.new('RGB', (100, 100), (255, 0, 0))
img.save('/tmp/red-square.png')
"
```

**2단계: document.json에 이미지 노드 추가**

루트 프레임의 `children` 배열에 이미지 노드의 `stable_id`를 추가하고,
`nodes` 배열에 이미지 노드를 추가합니다:

```json
{
  "stable_id": "photo-01",
  "name": "Hero Image",
  "visible": true,
  "opacity": 1.0,
  "blend_mode": "Normal",
  "transform": {
    "a": 1.0, "b": 0.0,
    "c": 0.0, "d": 1.0,
    "tx": 50.0, "ty": 80.0
  },
  "type": "image",
  "visual": {},
  "source": {
    "type": "linked",
    "path": "/tmp/red-square.png"
  },
  "width": 200.0,
  "height": 150.0
}
```

**핵심 필드 설명:**

| 필드 | 설명 | 필수 |
|------|------|------|
| `stable_id` | 고유 식별자 (나노ID 형태 권장) | 필수 |
| `type` | 반드시 `"image"` | 필수 |
| `source.type` | `"linked"` 또는 `"embedded"` | 필수 |
| `source.path` | 이미지 파일의 절대 경로 (linked일 때) | 조건부 |
| `width` / `height` | 화면에 표시할 크기 (px). 원본 크기와 다르면 자동 스케일링 | 필수 |
| `transform.tx` / `ty` | 노드의 x, y 위치 | 위치 지정 시 |
| `visual` | 빈 객체 `{}` 가능. 채움/테두리 등 추가 시각 속성 | 선택 |

**3단계: 부모 프레임의 children에 등록**

```json
{
  "stable_id": "root-frame",
  "type": "frame",
  "container": {
    "children": ["existing-node-01", "photo-01"]
  }
}
```

**4단계: 빌드**

```bash
ode build my-design.json --out output.png
```

### 레시피 B: CLI로 이미지 추가

빠른 프로토타이핑에 적합합니다. `ode add` 명령으로 이미지 노드를 추가합니다.

```bash
# 기본 사용법
ode add my-design.json image \
  --name "Photo" \
  --src /tmp/photo.png \
  --width 300 --height 200 \
  --parent root-frame

# 위치 지정
ode add my-design.json image \
  --name "Logo" \
  --src /absolute/path/to/logo.png \
  --width 120 --height 40 \
  --x 50 --y 20 \
  --parent header-frame
```

**CLI 제약사항:**

- `--width`와 `--height`는 필수 (이미지 크기를 반드시 명시)
- `--src`는 경로를 `ImageSource::Linked`로 저장 (파일 복사 안 함, 경로 검증 안 함)
- 회전, 스큐(transform의 a,b,c,d)는 CLI로 설정 불가 (document.json 직접 편집 필요)
- `--src` 없이 추가하면 source가 null인 빈 이미지 노드 생성

**이미지 속성 변경:**

```bash
# 위치 변경
ode set my-design.json photo-01 --x 100 --y 200

# 크기 변경
ode set my-design.json photo-01 --width 400 --height 300
```

### 레시피 C: 언팩 → 이미지 복사 → 팩

self-contained `.ode` 파일을 만들어야 할 때 사용합니다.
이 워크플로우는 이미지를 `.ode` ZIP 파일 안에 포함시킵니다.

**1단계: 기존 .ode 파일 언팩 (또는 새로 생성)**

```bash
# 기존 .ode 파일을 언팩
ode unpack my-design.ode --out my-design/

# 또는 언팩 디렉토리에서 시작 (ode init 등)
```

결과 디렉토리 구조:

```
my-design/
├── document.json
└── assets/          ← 이 폴더에 이미지를 넣음
```

**2단계: 이미지를 assets/에 해시 파일명으로 복사**

AssetStore는 `SHA-256` 해시 앞 16자를 파일명으로 사용합니다.
에이전트가 직접 해시를 계산해야 합니다:

```bash
# SHA-256 해시 앞 16자 계산
HASH=$(shasum -a 256 /tmp/photo.png | cut -c1-16)
# 확장자 포함하여 assets/에 복사
cp /tmp/photo.png my-design/assets/${HASH}.png
echo "파일명: assets/${HASH}.png"
```

**3단계: document.json에서 source.path를 assets/ 경로로 설정**

```json
{
  "source": {
    "type": "linked",
    "path": "assets/a1b2c3d4e5f6g7h8.png"
  }
}
```

경로가 `assets/`로 시작하면 렌더러가 AssetStore에서 해시로 조회합니다.
해시 부분(확장자 앞)이 정확히 일치해야 합니다.

**4단계: 언팩 상태에서 빌드 확인**

```bash
ode build my-design/ --out test-output.png
```

**5단계: 팩**

```bash
ode pack my-design/ --out my-design.ode
```

**6단계: 팩드 파일에서 빌드 확인**

```bash
ode build my-design.ode --out final-output.png
```

**라운드트립 검증:** `ode unpack` → `ode pack` → `ode unpack` 사이클에서
이미지가 손실 없이 유지됩니다.


## 완전한 예시: document.json

이미지가 포함된 최소 document.json 전체 예시:

```json
{
  "name": "Image Demo",
  "canvas": { "width": 800, "height": 600 },
  "views": [
    {
      "name": "Main",
      "kind": { "type": "canvas" },
      "root": "root"
    }
  ],
  "nodes": [
    {
      "stable_id": "root",
      "name": "Root Frame",
      "visible": true,
      "opacity": 1.0,
      "blend_mode": "Normal",
      "transform": {
        "a": 1.0, "b": 0.0,
        "c": 0.0, "d": 1.0,
        "tx": 0.0, "ty": 0.0
      },
      "type": "frame",
      "width": 800.0,
      "height": 600.0,
      "visual": {
        "fills": [
          {
            "paint": {
              "Solid": {
                "color": { "Raw": { "Srgb": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } } }
              }
            },
            "opacity": { "Raw": 1.0 },
            "blend_mode": "Normal",
            "visible": true
          }
        ]
      },
      "container": {
        "children": ["photo-01"]
      }
    },
    {
      "stable_id": "photo-01",
      "name": "My Photo",
      "visible": true,
      "opacity": 1.0,
      "blend_mode": "Normal",
      "transform": {
        "a": 1.0, "b": 0.0,
        "c": 0.0, "d": 1.0,
        "tx": 100.0, "ty": 50.0
      },
      "type": "image",
      "visual": {},
      "source": {
        "type": "linked",
        "path": "/absolute/path/to/photo.png"
      },
      "width": 400.0,
      "height": 300.0
    }
  ]
}
```

빌드:

```bash
ode build document.json --out result.png
ode build document.json --out result.svg --format svg
ode build document.json --out result.pdf --format pdf
```


## 주의사항 / 알려진 제약

### 지원 포맷

| 포맷 | 저장 | 렌더링 | 내보내기 (PNG/SVG/PDF) |
|------|------|--------|----------------------|
| **PNG** | O | O | O |
| **JPEG** | O | O | O |
| WebP | O (경로 저장) | X (image crate feature 미활성) | X |
| GIF | O (경로 저장) | X (image crate feature 미활성) | X |

**결론:** PNG 또는 JPEG만 사용하세요.

### ImageFillMode 미구현

데이터 모델에 `Paint::ImageFill`과 `ImageFillMode`(Fill, Fit, Crop, Tile)가
정의되어 있지만, 렌더링 파이프라인(convert.rs)에서 skip됩니다.
이미지를 프레임의 배경으로 채우는 기능은 아직 동작하지 않습니다.
이미지를 표시하려면 반드시 **image 노드**를 사용하세요.

### 경로 해석 규칙

렌더러(`emit_image`)는 `source.path`를 다음 규칙으로 해석합니다:

| 경로 패턴 | 해석 방식 |
|-----------|----------|
| `assets/{hash}.{ext}` | AssetStore에서 `{hash}`로 조회 (`get_loaded()`) |
| 기타 모든 경로 | `std::fs::read()`로 디스크에서 직접 읽기 |

- `assets/` 경로의 해시가 AssetStore에 없으면: **silent skip** (렌더링되지 않음)
- 외부 경로의 파일이 존재하지 않으면: **silent skip** (렌더링되지 않음)
- 상대 경로는 CWD 기준으로 해석됩니다
- **절대 경로 사용을 강력히 권장합니다** (환경 독립성)

### 에러 핸들링 (Silent Failure)

이미지 렌더링의 모든 실패는 **에러 메시지 없이 조용히 건너뜁니다**:

- 파일을 찾을 수 없음 → 이미지 없이 렌더링 계속
- 이미지 디코딩 실패 → 이미지 없이 렌더링 계속
- AssetStore에 해시 없음 → 이미지 없이 렌더링 계속

**디버깅 팁:** 이미지가 렌더링되지 않으면:
1. 경로가 정확한지 확인 (`ls -la <path>`)
2. 파일이 유효한 PNG/JPEG인지 확인 (`file <path>`)
3. assets/ 경로라면 해시가 정확한지 확인 (`shasum -a 256 <original> | cut -c1-16`)

### CLI로 설정 불가한 속성

다음 속성은 document.json을 직접 편집해야 합니다:

- **회전/스큐:** `transform`의 `a`, `b`, `c`, `d` 값
- **이미지 소스 변경:** 기존 노드의 `source` 교체
- **투명도:** `opacity` 값 조정
- **블렌드 모드:** `blend_mode` 변경

### 이미지 스케일링

`width`/`height`는 화면 표시 크기이며, 원본 이미지 크기와 다르면 자동으로 스케일링됩니다.
비율은 유지되지 않고 지정된 width/height에 맞춰 늘어나므로,
원본 비율을 유지하려면 직접 계산해야 합니다:

```python
# 원본 비율 유지 계산 예시
original_w, original_h = 1920, 1080
target_w = 400
target_h = target_w * (original_h / original_w)  # = 225.0
```


## 코드 수정 이력

### render.rs: 이미지 transform 합성 순서 수정

**파일:** `crates/ode-core/src/render.rs`

**수정 전:**
```rust
let combined = transform.post_concat(scale);
```

**수정 후:**
```rust
let combined = scale.post_concat(*transform);
```

**이유:** tiny-skia의 `post_concat`는 `self * other` 순서로 행렬을 합성합니다.
이미지 렌더링에서는 먼저 원본 크기를 표시 크기로 스케일링(scale)한 뒤
노드의 월드 좌표로 변환(transform)해야 합니다.
수정 전 순서에서는 이미지 소스 크기와 노드 크기가 다를 때 위치가 왜곡되었습니다.
