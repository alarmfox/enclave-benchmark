import re
import sys

data = sys.stdin.read()

math_regex = r'<math display="inline">(.*?)<\/math>'
figure_regex = r'figures(\/ch[1-3])?\/(.*)'
synthl_open = r'<syntaxhighlight lang=".*">'
synthl_close = r'<\/syntaxhighlight>'

r = re.sub(math_regex, r"''\1''", data)
r = re.sub(figure_regex, r"\2", r)
r = re.sub(synthl_open, r"<pre>", r)
r = re.sub(synthl_close, r"</pre>", r)

print(r)

