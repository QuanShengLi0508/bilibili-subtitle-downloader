"""Build the static PDF font from the repository's OFL-licensed Noto Sans SC.
Unique glyphs for Unicode aliases preserve text when printpdf builds ToUnicode.
"""
from pathlib import Path
from copy import deepcopy
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
root = Path(__file__).resolve().parents[1]
font = TTFont(root / 'mobile/assets/NotoSansSC.ttf')
instantiateVariableFont(font, {'wght': 400}, inplace=True)
font.ensureDecompiled()
order = list(font.getGlyphOrder())
seen = set()
aliases = {}
for code, glyph in sorted(font.getBestCmap().items()):
    if glyph not in seen:
        seen.add(glyph)
        continue
    name = f'shiwen_uni{code:06X}'
    font['glyf'].glyphs[name] = deepcopy(font['glyf'][glyph])
    font['hmtx'].metrics[name] = font['hmtx'].metrics[glyph]
    if 'vmtx' in font:
        font['vmtx'].metrics[name] = font['vmtx'].metrics[glyph]
    order.append(name)
    aliases[code] = name
font.setGlyphOrder(order)
for table in font['cmap'].tables:
    if table.isUnicode() and hasattr(table, 'cmap'):
        for code, name in aliases.items():
            if code in table.cmap:
                table.cmap[code] = name
for name in font['name'].names:
    if name.nameID in (1, 3, 4, 6, 16):
        value = 'ShiwenSans-Regular' if name.nameID in (3, 6) else 'Shiwen Sans'
        name.string = value.encode(name.getEncoding())
output = root / 'assets/fonts'
output.mkdir(exist_ok=True)
font.save(output / 'ShiwenSans-Regular.ttf')
(output / 'OFL.txt').write_bytes((root / 'mobile/assets/NotoSansSC-OFL.txt').read_bytes())
print(f'Prepared PDF font with {len(aliases)} distinct Unicode aliases')
