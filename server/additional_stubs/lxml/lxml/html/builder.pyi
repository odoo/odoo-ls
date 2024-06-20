from functools import partial

from ..builder import ElementMaker
from ._element import HtmlElement
from ._form import (
    FormElement,
    InputElement,
    LabelElement,
    SelectElement,
    TextareaElement,
)

E: ElementMaker[HtmlElement]

# Use inferred type, value is not important in stub
A = E.a
ABBR = E.abbr
ACRONYM = E.acronym
ADDRESS = E.address
APPLET = E.applet
AREA = E.area
B = E.b
BASE = E.base
BASEFONT = E.basefont
BDO = E.bdo
BIG = E.big
BLOCKQUOTE = E.blockquote
BODY = E.body
BR = E.br
BUTTON = E.button
CAPTION = E.caption
CENTER = E.center
CITE = E.cite
CODE = E.code
COL = E.col
COLGROUP = E.colgroup
DD = E.dd
DEL = E.__getattr__("del")
DFN = E.dfn
DIR = E.dir
DIV = E.div
DL = E.dl
DT = E.dt
EM = E.em
FIELDSET = E.fieldset
FONT = E.font
FORM: partial[FormElement]
FRAME = E.frame
FRAMESET = E.frameset
H1 = E.h1
H2 = E.h2
H3 = E.h3
H4 = E.h4
H5 = E.h5
H6 = E.h6
HEAD = E.head
HR = E.hr
HTML = E.html
I = E.i
IFRAME = E.iframe
IMG = E.img
INPUT: partial[InputElement]
INS = E.ins
ISINDEX = E.isindex
KBD = E.kbd
LABEL: partial[LabelElement]
LEGEND = E.legend
LI = E.li
LINK = E.link
MAP = E.map
MENU = E.menu
META = E.meta
NOFRAMES = E.noframes
NOSCRIPT = E.noscript
OBJECT = E.object
OL = E.ol
OPTGROUP = E.optgroup
OPTION = E.option
P = E.p
PARAM = E.param
PRE = E.pre
Q = E.q
S = E.s
SAMP = E.samp
SCRIPT = E.script
SELECT: partial[SelectElement]
SMALL = E.small
SPAN = E.span
STRIKE = E.strike
STRONG = E.strong
STYLE = E.style
SUB = E.sub
SUP = E.sup
TABLE = E.table
TBODY = E.tbody
TD = E.td
TEXTAREA: partial[TextareaElement]
TFOOT = E.tfoot
TH = E.th
THEAD = E.thead
TITLE = E.title
TR = E.tr
TT = E.tt
U = E.u
UL = E.ul
VAR = E.var

# attributes
ATTR = dict

def CLASS(v: str) -> dict[str, str]: ...
def FOR(v: str) -> dict[str, str]: ...
