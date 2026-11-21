# The Tess Programming Language

> 이 문서는 **바이브코딩용 AI**에게 Tess 를 가르치기 위한 문서입니다. AI 는 이 문서만 읽고
> Tess 코드를 쓴 뒤 `tess-ent` 로 검사·변환하면 됩니다. 전체 명세는 `tess-language.md`(언어)와
> `tess-compile.md`(컴파일 동작)에 있습니다. 원본: [ddtt786/tess](https://github.com/ddtt786/tess) @ 97d2266 (MIT).

## 1. Tess 란

Tess 는 엔트리(playentry.org) 블록 코드로 컴파일되는 **텍스트 언어**입니다. 사람이 블록을 끌어다
놓는 대신 글로 작품을 쓰면, 컴파일러가 엔트리 블록으로 바꿔 `.ent` 파일을 만듭니다.

```
Tess 소스(.tess) --tess-ent build--> .ent --> playentry.org 에서 불러오기
```

핵심 규칙 세 가지만 기억하세요.

1. **블록은 열고 `end` 로 닫습니다.** 여는 말은 `:` · `do` · `then` 중 아무거나(뜻 같음). 들여쓰기는 의미가 없습니다.
2. **오브젝트·장면·신호·소리·모양 이름은 따옴표 문자열**로 가리킵니다 (`touching("적")`, `jump "끝"`, `send "시작"`).
3. **변수·리스트의 초기값은 상수만** 됩니다. 계산이 필요하면 `when start` 안에서 대입합니다.

주석은 `#` 입니다 (`#ff0000` 같은 색상 리터럴은 주석이 아님).

## 2. 작업 순서 (AI 용)

```bash
tess-ent ref                         # 언어 레퍼런스 전체 읽기 (필요할 때)
tess-ent check main.tess --json      # 에러를 줄·칸과 함께 받기 -> 고치고 반복
tess-ent build main.tess -o game.ent # 변환 (--assets 폴더, --strict 지원)
```

- `errors` 는 반드시 0 이 될 때까지 고칩니다. `warnings` 는 읽고 판단합니다.
- `notices` 는 "실행할 때 이름으로 찾습니다" 같은 안내라 실패가 아닙니다.
- 모양·소리 파일은 **소스 파일 기준 상대 경로**(또는 `--assets` 폴더)에 실제로 있어야 크기·길이가 정확히 기록됩니다. 없으면 `size W H` / `for 초` 로 직접 적을 수 있습니다.
- MCP 로 연결했다면 `tess_reference` → `tess_check` → `tess_build` 순서로 부르면 됩니다.

## 3. 파일의 뼈대

```tess
project:
  title "작품 제목"
  description "설명"
  fps 60
end

var 점수 = 0                 # 전역 변수
list 기록 = [1, 2, 3]        # 전역 리스트

function 알리기(내용):
  say 내용 for 1
end

scene "무대":
  object "고양이":
    default costume 기본 "cat.png"
    costume 달리기 "cat_run.png"
    sound 야옹 "meow.mp3"
    x = 0
    y = -50

    when start do
      say "안녕!" for 2
    end
  end

  text "점수판":
    text_content = "점수: 0"
    font_size = 24
  end
end
```

| 요소 | 뜻 |
| --- | --- |
| `project:` | 제목·설명·fps (파일당 하나, 생략 가능) |
| `scene "id":` | 장면. 안에 `object`/`text`. 표시 이름은 `name "..."` |
| `object "id":` | 그림 오브젝트. 속성(`x`, `y`, `size`, `visible` …)·`costume`·`sound`·지역 `var`/`list`/`function`·`when` 블록 |
| `text "id":` | 글상자. `text_content`, `font`, `font_size`, `font_color`, `bg_color`, `text_align`, `text_bold` … |
| `function 이름(a, b):` | 함수 |
| `var` / `list` | 전역(최상위) · 오브젝트 지역(오브젝트 안) · 함수 지역(함수 안, `list` 불가) |
| `use "파일.tess"` | 다른 파일을 그 자리에 붙여 넣기 (`useobject`/`usetext` 는 오브젝트로 감싸서) |

장면을 하나도 안 쓰면 "장면 1" 이 자동으로 만들어지고, 최상위에 둔 `object` 는 첫 장면에 들어갑니다.

## 4. 값과 연산

- 숫자 `42` `3.14` · 문자열 `"안녕"` · `true`/`false` · 색상 `#ff0000` `#abc` `#검정` · `transparent`
- 연산자(낮음→높음): `or` < `and` < `not` < 비교(`== != > < >= <=`) < `+ -` < `* / % //` < `**` < 단항 `-`
- `+` 는 문자열 이어 붙이기도 됩니다. `//` 는 몫, `%` 는 나머지, `**` 의 지수는 상수여야 합니다.
- 대입: `=` `+=` `-=` `*=` `/=` `%=` `**=`
- 인덱스는 **1부터**: `기록[1]`, `"문자"[2]`
- **공백으로 인자를 늘어놓는 명령**(`move`, `go`)은 인자에 이항식을 직접 못 씁니다 → 괄호: `move (a + b) 10`
- `true`/`false` 를 값으로 저장하면 문자열 `"TRUE"`/`"FALSE"` 입니다.

## 5. 문장 한눈에 보기

### 흐름

```tess
if 점수 > 90: say "A" else: say "B" end
repeat 5: forward 10 end
while 체력 > 0: wait 0.5 end
until touching("골"): forward 5 end
forever: ... end
wait 1                 # 초   (wait 조건식  → 참이 될 때까지)
break  continue  skip  restart  stop
stop me | stop other | stop them | stop all       # 스크립트 멈추기
```

### 이벤트 (오브젝트 안, 전부 병렬 실행)

```tess
when start do ... end
when scene start do ... end
when key "space" do ... end          when key "space" up do ... end
when click do ... end                when click up do ... end
when stage click do ... end          when stage click up do ... end
when signal "게임 시작" do ... end
when cloned do ... end
```

키 이름: `"a"`~`"z"`, `"0"`~`"9"`, `"space"` `"enter"` `"esc"` `"left"` `"right"` `"up"` `"down"` 등.

### 움직임·모양·말

```tess
x = 100   y += 10   forward 10   forward 10 at 90   bounce
move 20 20   move 50 0 in 1          # 상대 이동 (in 초)
go 0 0   go "mouse"   go "적" in 2    # 절대 이동
angle = 90   way = 180   turn 45   steer 30   look "mouse"
show   hide   costume = "달리기"   costume = 2   next costume   prev costume
size = 120   scale_x = 80   reset size
effect_color = 50   effect_brightness += 10   effect_alpha = 30   clear effects
flip x   flip y   order first|front|back|last
say "안녕" for 2   think "음…"   clear bubble
```

### 신호·복제·장면

```tess
send "시작"          # 보내고 바로 진행
call "시작"          # 받는 스크립트가 끝날 때까지 기다림
clone   clone "적"   kill   del clones
jump "다음"   jump next   jump back
```

### 붓·소리·글상자

```tess
draw_color = #ff0000   draw_width = 5   draw_alpha = 0   fill_color = #0000ff
start draw  stop draw  start fill  stop fill  stamp  clear draw

play sound "야옹"   play sound "야옹" and wait   play sound "야옹" for 2   play sound "야옹" from 1 to 3
play bgm "음악"   stop bgm   stop sound this   stop sound all
sound_volume = 80   sound_speed = 1.2

write "내용"   append "뒤에"   prepend "앞에"   clear text      # text 오브젝트 전용
```

### 데이터

```tess
in 기록 add 90        in 기록 insert 100 at 2        remove 기록[1]
기록[1] = 95          var 개수 = length(기록)          var 있나 = contains(기록, 100)
ask "이름은?"         say answer
show 점수   hide 점수   show timer   hide answer
start timer   stop timer   reset timer
```

테이블(엔트리 자료 분석):

```tess
table 성적표 as "성적 표":
  columns "이름", "점수"
  row "철수", 90
  row "영희", 95
end
```

## 6. 값으로 쓰는 것

- **판단**: `key_down("left")`, `touching("적")`, `touching("wall")`, `mouse_down`, `clicked`
- **상태 값**: `timer`, `answer`, `block_count`, `user_id`, `nickname`
- **객체 정보**: `x`, `y`, `angle`, `way`, `size`, `costume`, `costume_number` (괄호 없으면 자기 자신, `x("적")`, `x("mouse")`), `distance("대상")`
- **수학**: `sin cos tan asin acos atan`(도 단위) `log2 ln log10 floor ceil round abs factorial random(a,b) root(값,n)`
- **문자열**: `length slice(s,from,to) count join index_of replace reverse uppercase lowercase`
- **시간·색**: `now("year")`(year/month/day/hour/minute/second/weekday), `to_hex(r,g,b)`, `from_hex(#색, red)`

## 7. 함수

```tess
function 두배(값):           # 마지막 줄이 return -> 값 함수 (식으로만 씀)
  var 배수 = 2
  return 값 * 배수
end

function 점수_더하기(추가):    # return 없음 -> 일반 함수 (문장으로만 씀)
  점수 += 추가
end

function 스폰(이름, 살았나?):   # 이름 뒤 ? 는 엔트리의 판단 칸
  if 살았나: say 이름 end
end
```

- `return` 은 **마지막 문장**에서만, 한 번만 쓸 수 있습니다.
- 함수는 **오브젝트 지역 변수를 직접 읽지 못합니다** → 매개변수로 넘기세요. 전역 변수는 읽고 쓸 수 있습니다.
- `object` 안에 선언한 함수는 그 오브젝트에서만 호출할 수 있습니다.
- 값 함수를 문장으로, 일반 함수를 식으로 쓰면 에러입니다.

## 8. 자주 나는 에러와 해결

| 증상 | 원인 → 해결 |
| --- | --- |
| "변수·리스트의 초기값은 상수여야 합니다" | `var a = random(1,5)` → `var a = 0` 후 `when start` 에서 대입 |
| "선언되지 않은 이름" | 변수를 먼저 `var` 로 선언 (이름 오타는 "혹시 …?" 힌트가 나옴) |
| "return 은 마지막 문장에서만" | 중간 `return` 제거, `if/else` 로 값을 변수에 담고 마지막에 `return 변수` |
| 오브젝트에 모양이 없다는 경고 | `default costume 이름 "파일.png"` 추가 (파일이 없으면 `size W H` 병기) |
| `move 50 -30` 이 이상하게 동작 | `move 50 (-30)` 또는 `move (a + b) 10` 처럼 괄호 사용 |
| 글상자에 `costume`/붓, 오브젝트에 `write` | `write`/`font*` 는 `text` 전용, 붓·`costume` 은 `object` 전용 |
| 장면·신호 이름 오류 | `jump`/`send` 의 이름은 **문자열 리터럴**이어야 하고, 장면은 실제로 존재해야 함 |
| "알 수 없는 키 이름" | 위 키 이름 목록 참고 (`"space"`, `"left"` …) |

## 9. 완성 예제

```tess
project:
  title "고양이 달리기"
  fps 60
end

var 점수 = 0

scene "무대":
  object "고양이":
    default costume 기본 "cat.png" size 100 100
    x = 0
    y = -80

    when start do
      점수 = 0
      forever:
        if key_down("right"):
          x += 5
        end
        if key_down("left"):
          x -= 5
        end
        if touching("별"):
          점수 += 1
          send "별 먹음"
        end
        wait 0.02
      end
    end
  end

  object "별":
    default costume 기본 "star.png" size 40 40
    y = 120

    when start do
      forever:
        y -= 4
        if y < -120:
          y = 120
          x = random(-200, 200)
        end
        wait 0.02
      end
    end

    when signal "별 먹음" do
      y = 120
      x = random(-200, 200)
    end
  end

  text "점수판":
    text_content = "점수: 0"
    font_size = 24
    x = -180
    y = 110

    when start do
      forever:
        write join("점수: ", 점수)
        wait 0.1
      end
    end
  end
end
```

## 10. 쓰기 전 체크리스트

- [ ] 모든 블록(`if` `repeat` `forever` `when` `function` `object` `scene` …)이 `end` 로 닫혔는가
- [ ] 이름(오브젝트·장면·신호·소리·모양)을 따옴표 문자열로 썼는가
- [ ] 변수·리스트 초기값이 상수인가
- [ ] 함수의 `return` 이 마지막 문장 하나뿐인가
- [ ] `check --json` 의 `errors` 가 비었는가
