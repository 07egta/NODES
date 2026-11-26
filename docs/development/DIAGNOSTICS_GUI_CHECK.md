# Native diagnostic recovery check

Executed on 2026-11-27 JST in the Linux/Xfce desktop, using the compact-debug
NODES binary from this workstream. This is a manual application check, separate
from unit tests and CI. It is not a Windows/macOS or geometric-output test.

## Reproducible input

Open [`tests/fixtures/diagnostics/two-roots.morph`](../../tests/fixtures/diagnostics/two-roots.morph)
in NODES. This fixture intentionally contains runtime errors and therefore lives
outside the successful `examples` collection. It is structurally valid and has
six nodes:

| ID | Node | Initial result |
| --- | --- | --- |
| 0 | Divide, 半径の計算, A=12 / B=0 | Independent division error |
| 1 | Divide, 高さの計算, A=20 / B=0 | Independent division error |
| 2 | Add, 寸法を合計, inputs 0 and 1 | Blocked by either upstream root |
| 3 | Multiply, 仕上げ係数, input 2 multiplied by 2 | Blocked downstream |
| 4 | Number, 別系統の正常値, value=10 | Successful independent branch |
| 5 | Disabled Divide, 無効にした試作 | Disabled; not a root failure |

The real console command `nodes-cli diagnose <fixture>` was also observed to
return exit code 1 with schema 1, two roots, two blocked nodes, four failed nodes,
one successful node and one disabled node. The diagnostic operation leaves the
source document unchanged.

## Observed workflow

1. Launched the native application and confirmed all six nodes and runtime
   errors rendered. Opened the status-bar **診断** button.
2. The **入力とエラー** view displayed two root candidates. Each root reported two
   downstream failures; the blocked section counted two nodes rather than four.
   The disabled section counted one.
3. Clicked the `#00 半径の計算` link. The canvas fitted/selected that node and the
   inspector showed the correct Divide node, ID and original division error.
4. Changed B from 0 to 35 using its inspector slider. The diagnostic view updated
   to one remaining root (`#01 高さの計算`) and the status bar to three failures.
5. Clicked that remaining root and changed its B from 0 to 35. The diagnostic
   window changed to **計算エラーはありません**. The disabled node remained listed
   and the worker reported three computed plus two reused enabled nodes.
6. Opened **処理時間**. It showed three computed nodes. Enabling the cache checkbox
   showed all five enabled nodes, with the additional two labeled **再利用**.
   The disabled node did not become a measured active node.

Each stage was visually inspected through native screenshots. Screenshot files
are held by the desktop session and are not included in this repository. The
fixture and these exact steps make the check reproducible after that session ends.

## Limits and remaining checks

- The name/ID filter, 101st-row paging and live-renaming behavior have automated
  logic tests. They were not typed through during this GUI check because bound
  keyboard input did not reach the native numeric field in this environment.
  Pointer/sliders worked after moving the pointer before clicking. This is an
  automation limitation; it does not establish an application keyboard defect.
- No numeric editing, clipboard, screen-reader or touch accessibility claim is
  made from this pointer-driven check.
- Timing values on this small graph are transient measurements, not a benchmark
  or a performance guarantee.
- Geometry preview, printability and fabrication remain separate verification
  tasks. This fixture contains only numeric values.
