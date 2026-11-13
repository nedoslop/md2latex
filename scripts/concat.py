import os
import sys

cwd = os.path.dirname(os.path.dirname(__file__))

files = [
    'src/config.rs',
    'src/images.rs',
    'src/main.rs',
    'src/markdown.rs',
    'src/template.rs',
    'templates/base.tex.tera',
    'templates/title_course.tex.tera',
    'md2latex.toml',
    'examples/input.md',
]

out = open('joined.txt', 'w', encoding='utf-8')
ret = ''

for fn in files:
    ret += 'FILE: ' + fn + '\n'
    for line in open(os.path.join(cwd, fn), encoding='utf-8').read().split('\n'):
        ret += line + '\n'

out.write(ret)
