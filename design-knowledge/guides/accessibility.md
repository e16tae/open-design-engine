---
id: accessibility
name: 접근성
layer: accessibility
contexts: [web, mobile-app, print, presentation]
related: [design-principles, spatial-composition]
---

# 접근성 (Accessibility) 디자인 가이드

이 가이드는 디자인 에이전트가 접근성 기준을 충족하는 디자인을 생성하기 위한 지침을 제공합니다.
WCAG 2.1 AA 수준을 기본 기준으로 삼으며, 플랫폼별 접근성 가이드라인을 통합합니다.


## 핵심 원칙

접근성 디자인은 WCAG의 4가지 원칙(POUR)을 기반으로 합니다.

### 1. 인식 가능 (Perceivable)

모든 정보와 UI 컴포넌트는 사용자가 인식할 수 있는 방식으로 제공되어야 합니다.

- 텍스트와 배경 사이의 충분한 색상 대비를 유지합니다.
- 색상만으로 정보를 전달하지 않습니다 (패턴, 아이콘, 텍스트 레이블 병행).
- 이미지에는 대체 텍스트 설명이 필요합니다.
- 텍스트 크기는 사용자가 조절할 수 있어야 합니다.

### 2. 운용 가능 (Operable)

모든 UI 컴포넌트와 내비게이션은 조작 가능해야 합니다.

- 터치 타겟은 충분한 크기를 확보합니다 (최소 44pt).
- 키보드로 모든 기능에 접근할 수 있어야 합니다.
- 포커스 인디케이터가 명확해야 합니다.
- 충분한 타겟 간 간격을 유지합니다.

### 3. 이해 가능 (Understandable)

정보와 UI 조작이 이해 가능해야 합니다.

- 일관된 내비게이션 패턴을 사용합니다.
- 예측 가능한 인터랙션을 설계합니다.
- 에러 메시지는 명확하고 구체적이어야 합니다.
- 폼 레이블은 항상 가시적이어야 합니다.

### 4. 견고한 (Robust)

콘텐츠는 다양한 보조 기술이 해석할 수 있도록 견고해야 합니다.

- 시맨틱한 구조를 사용합니다 (헤딩 계층, 랜드마크).
- 상태 변화를 프로그래밍 방식으로 전달합니다.
- 커스텀 컴포넌트에도 접근성 속성을 부여합니다.


## 규칙

### 1. 색상 대비 (Color Contrast)

WCAG 2.1 Success Criterion 1.4.3 (AA 수준) 기준:

| 텍스트 유형 | 최소 대비율 | 비고 |
|---|---|---|
| 일반 텍스트 (< 18pt / < 14pt bold) | **4.5:1** | 본문, 레이블, 캡션 등 |
| 큰 텍스트 (>= 18pt / >= 14pt bold) | **3:1** | 헤딩, 대형 버튼 텍스트 |
| UI 컴포넌트, 그래픽 | **3:1** | 아이콘, 보더, 포커스 링 |
| 장식적 텍스트 | 없음 | 순수 장식적 요소는 면제 |

**좋은 예:**
- 흰 배경(#FFFFFF)에 검은 텍스트(#000000): 21:1
- 흰 배경(#FFFFFF)에 진회색 텍스트(#595959): 7:1
- 어두운 배경(#1A1A1A)에 밝은 텍스트(#E0E0E0): 12.6:1

**나쁜 예:**
- 흰 배경(#FFFFFF)에 밝은 회색(#CCCCCC): 1.6:1
- 밝은 파랑(#89CFF0)에 흰 텍스트(#FFFFFF): 1.5:1

### 2. 터치 타겟 크기 (Touch Target Size)

모바일 인터페이스에서 터치 가능한 요소의 최소 크기:

| 플랫폼 | 최소 크기 | 권장 크기 |
|---|---|---|
| iOS (Apple HIG) | **44 x 44 pt** | 44 x 44 pt |
| Android (Material) | **48 x 48 dp** | 48 x 48 dp |
| Web (WCAG 2.5.5 AAA) | **44 x 44 CSS px** | 48 x 48 CSS px |
| Web (WCAG 2.5.8 AA) | **24 x 24 CSS px** | 44 x 44 CSS px |

- 인접한 터치 타겟 사이에 최소 8px 간격을 유지합니다.
- 타겟이 작더라도 터치 영역(hit area)은 최소 크기를 충족해야 합니다.
- 아이콘 버튼은 시각적으로 작더라도 터치 영역은 44pt 이상이어야 합니다.

### 3. 글꼴 크기 (Font Size)

| 컨텍스트 | 최소 크기 | 권장 크기 | 비고 |
|---|---|---|---|
| Web 본문 | **16px** | 16-18px | 브라우저 기본값 |
| Web 보조 텍스트 | **12px** | 14px | 캡션, 각주 등 |
| Mobile 본문 | **14px** | 16-17px | iOS: 17pt, Android: 16sp |
| Mobile 보조 텍스트 | **12px** | 14px | 세컨더리 정보 |
| Print 본문 | **10pt** | 11-12pt | 물리적 포인트 단위 |

- 줄 높이(line-height)는 글꼴 크기의 1.4-1.6배를 권장합니다.
- 자간(letter-spacing)은 기본값 이상을 유지합니다.
- 단락 간격(paragraph-spacing)은 글꼴 크기의 1.5배 이상을 권장합니다.

### 4. 색상만의 정보 전달 금지 (Color-Only Information)

WCAG 1.4.1 기준: 색상이 유일한 시각적 구분 수단이어서는 안 됩니다.

- 에러 상태: 빨간 색 + 아이콘 + 텍스트 메시지 병행
- 차트 데이터: 색상 + 패턴(점선, 빗금 등) 병행
- 링크: 색상 + 밑줄 또는 다른 시각적 구분
- 필수 필드: 색상 + 별표(*) 또는 텍스트 레이블

### 5. 포커스 인디케이터 (Focus Indicators)

WCAG 2.4.7 기준: 키보드 포커스를 받는 요소는 시각적 인디케이터가 있어야 합니다.

- 포커스 링은 최소 2px 두께의 아웃라인을 권장합니다.
- 포커스 링 색상은 배경과 3:1 이상의 대비를 유지합니다.
- 포커스 링과 요소 사이에 2px 오프셋을 권장합니다.
- 커스텀 포커스 스타일은 브라우저 기본 스타일보다 더 명확해야 합니다.

### 6. 인지 접근성 (Cognitive Accessibility)

- 한 화면에 표시되는 주요 액션은 1-3개로 제한합니다.
- 복잡한 폼은 단계별로 분할합니다 (wizard/stepper 패턴).
- 중요한 정보는 페이지 상단에 배치합니다.
- 긴 텍스트 블록은 최대 80자(영문) / 40자(한글) 너비로 제한합니다.
- 충분한 여백과 시각적 그룹핑을 사용합니다.

### 7. 국제화 (Internationalization)

- RTL(Right-to-Left) 레이아웃 지원: 아랍어, 히브리어 등
  - 텍스트 정렬, 아이콘 방향, 레이아웃 미러링
- CJK(Chinese, Japanese, Korean) 타이포그래피:
  - 세로쓰기(vertical writing mode) 고려
  - 한자 혼용 시 적절한 글꼴 크기 확보
  - 줄바꿈 규칙(line-break: strict) 적용
- 텍스트 확장 공간 확보:
  - 독일어는 영어 대비 30% 더 긴 텍스트
  - 버튼, 레이블에 여유 공간 필요


## 맥락별 적용

### Web

- 기본 글꼴 크기: 16px (rem 기반)
- 반응형 글꼴 크기 조정 지원 (zoom 200%까지)
- 키보드 내비게이션 + 포커스 관리 필수
- 색상 모드: 라이트/다크 모드 대비 동시 준수
- 스킵 내비게이션 링크 제공
- ARIA 랜드마크 구조 반영

### Mobile App

- 터치 타겟: iOS 44pt / Android 48dp
- 다이나믹 타입 지원 (iOS) / 글꼴 크기 설정 반영 (Android)
- 스크린 리더 순서 = 시각적 순서
- 제스처 대안 제공 (스와이프 대신 버튼)
- 시스템 접근성 설정 존중 (Reduce Motion, Bold Text 등)

### Print

- 물리적 포인트(pt) 기준 10pt 이상
- 흑백 인쇄 시에도 정보 전달 가능하도록 설계
- 충분한 마진과 거터(gutter) 확보
- 하이퍼링크는 URL 표기 병행

### Presentation

- 발표 환경 고려: 프로젝터 대비 감소 -> 대비율 상향 (7:1 이상 권장)
- 최소 글꼴 크기: 24pt (본문), 32pt (헤딩)
- 슬라이드당 핵심 포인트 3개 이하
- 애니메이션은 최소화, motion-reduce 고려


## ODE 매핑

ODE 포맷에서 접근성 관련 속성이 어떻게 표현되는지 설명합니다.

### 색상 대비 -> Fill colors + StyleValue

텍스트 노드의 전경색은 `fills` 배열의 `Solid` paint에서 가져옵니다.
배경색은 부모 프레임의 fills에서 가져옵니다.

```json
{
  "stable_id": "text-01",
  "name": "Body Text",
  "kind": {
    "type": "text",
    "data": {
      "content": "읽기 쉬운 텍스트",
      "default_style": {
        "font_size": { "Raw": 16.0 },
        "font_family": { "Raw": "Inter" },
        "font_weight": { "Raw": 400 }
      },
      "visual": {
        "fills": [
          {
            "paint": {
              "Solid": {
                "color": { "Raw": { "Srgb": { "r": 0.13, "g": 0.13, "b": 0.13, "a": 1.0 } } }
              }
            },
            "opacity": { "Raw": 1.0 },
            "blend_mode": "Normal",
            "visible": true
          }
        ]
      }
    }
  }
}
```

부모 프레임에 배경색을 지정합니다:

```json
{
  "stable_id": "card-01",
  "name": "Card Background",
  "kind": {
    "type": "frame",
    "data": {
      "width": 400.0,
      "height": 300.0,
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
        "children": ["text-01"]
      }
    }
  }
}
```

`contrast_ratio` 체커는 텍스트 노드의 첫 번째 visible solid fill 색상과
부모 프레임의 배경색을 비교하여 WCAG 대비율을 계산합니다.

### 터치 타겟 -> Frame width/height

인터랙티브 요소는 프레임 노드로 표현하며, width와 height로 크기를 설정합니다.

```json
{
  "stable_id": "btn-01",
  "name": "Submit Button",
  "kind": {
    "type": "frame",
    "data": {
      "width": 120.0,
      "height": 48.0,
      "container": {
        "children": ["btn-label-01"],
        "layout": {
          "direction": "Horizontal",
          "primary_axis_align": "Center",
          "counter_axis_align": "Center",
          "padding": { "top": 12.0, "right": 24.0, "bottom": 12.0, "left": 24.0 },
          "item_spacing": 8.0
        }
      }
    }
  }
}
```

`min_value` 체커는 `property: "width"` (또는 `"height"`) 파라미터로 프레임 크기를
검사하여 최소 터치 타겟 크기를 충족하는지 확인합니다.

### 글꼴 크기 -> TextStyle.font_size

텍스트 노드의 `default_style.font_size`에서 글꼴 크기를 읽습니다.

```json
{
  "stable_id": "caption-01",
  "name": "Caption",
  "kind": {
    "type": "text",
    "data": {
      "content": "보조 설명 텍스트",
      "default_style": {
        "font_size": { "Raw": 14.0 },
        "font_family": { "Raw": "Pretendard" },
        "font_weight": { "Raw": 400 },
        "line_height": { "Factor": 1.5 },
        "letter_spacing": { "Raw": 0.0 },
        "text_align": "Left",
        "vertical_align": "Top"
      }
    }
  }
}
```

`min_value` 체커는 `property: "font_size"` 파라미터로 텍스트 노드의 글꼴 크기를
검사하여 최소 가독 크기를 충족하는지 확인합니다.

### 디자인 토큰을 활용한 접근성 관리

색상과 글꼴 크기를 디자인 토큰으로 관리하면 일관된 접근성을 유지할 수 있습니다:

```json
{
  "tokens": {
    "collections": [
      {
        "name": "colors",
        "modes": [
          { "id": 1, "name": "light" },
          { "id": 2, "name": "dark" }
        ],
        "tokens": [
          {
            "name": "text.primary",
            "values": {
              "1": { "Direct": { "Color": { "Srgb": { "r": 0.13, "g": 0.13, "b": 0.13, "a": 1.0 } } } },
              "2": { "Direct": { "Color": { "Srgb": { "r": 0.93, "g": 0.93, "b": 0.93, "a": 1.0 } } } }
            }
          },
          {
            "name": "surface.primary",
            "values": {
              "1": { "Direct": { "Color": { "Srgb": { "r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0 } } } },
              "2": { "Direct": { "Color": { "Srgb": { "r": 0.07, "g": 0.07, "b": 0.07, "a": 1.0 } } } }
            }
          }
        ]
      }
    ]
  }
}
```

토큰을 사용하면 light/dark 모드 전환 시 자동으로 접근성 기준을 충족하는
색상 조합이 적용됩니다. 두 모드 모두에서 대비율을 검증해야 합니다.


## 안티패턴

디자인 에이전트가 자주 범하는 접근성 관련 실수 목록입니다.

### 1. 회색 텍스트 남용

**문제:** 시각적 계층을 만들기 위해 텍스트를 밝은 회색(#999, #AAA)으로 설정하여
대비율이 부족해짐.

**해결:** opacity를 낮추는 대신, 충분한 대비를 유지하는 색상을 사용합니다.
- 흰 배경에서 최소 #595959 (7:1) 또는 #767676 (4.5:1) 사용.

### 2. 장식적 폰트 크기

**문제:** 미적 이유로 본문 텍스트를 10px 이하로 설정.

**해결:** 12px 미만의 텍스트는 순수 장식 목적이 아닌 한 사용하지 않습니다.
본문은 16px, 보조 텍스트는 14px을 기본으로 합니다.

### 3. 색상만으로 상태 표현

**문제:** 성공(초록), 에러(빨강), 경고(노랑)를 색상만으로 구분.

**해결:** 아이콘, 텍스트 레이블, 보더 스타일 등 추가적인 시각 단서를 병행합니다.

### 4. 작은 터치 타겟

**문제:** 아이콘 버튼을 24x24px로 설정하여 터치하기 어려움.

**해결:** 시각적 크기가 작더라도 터치 영역(hit area)은 44pt 이상 확보합니다.
ODE에서는 프레임의 width/height를 44 이상으로 설정합니다.

### 5. 포커스 스타일 제거

**문제:** `outline: none`에 해당하는 스타일로 포커스 인디케이터를 숨김.

**해결:** 기본 포커스 스타일을 제거하는 대신, 브랜드에 맞는 커스텀 포커스
스타일을 설계합니다. 최소 2px 아웃라인 + 배경 대비 3:1 이상.

### 6. 고정 글꼴 크기

**문제:** px 단위로 고정된 글꼴 크기만 사용하여 사용자 설정을 무시.

**해결:** 상대 단위(rem, em)를 고려한 설계. ODE의 `StyleValue::TokenRef`를
활용하여 토큰 기반 크기 관리.

### 7. 불충분한 행간

**문제:** line-height를 1.0이나 1.2로 설정하여 읽기 어려움.

**해결:** 본문 텍스트의 line-height는 1.4-1.6 범위를 사용합니다.
ODE의 TextStyle에서 `line_height: { "Factor": 1.5 }`로 설정합니다.

### 8. 다크 모드 미검증

**문제:** 라이트 모드에서만 대비율을 확인하고, 다크 모드에서 확인하지 않음.

**해결:** `ode review`를 두 모드 모두에서 실행합니다.
`ode tokens set-mode`로 모드를 전환하면서 각각 검증합니다.
