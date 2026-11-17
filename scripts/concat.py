import os

cwd = os.path.dirname(os.path.dirname(__file__))

files = [
    'src/config.rs',
    'src/images.rs',
    'src/log.rs',
    'src/main.rs',
    'src/markdown.rs',
    'src/plantuml.rs',
    'src/template.rs',
    'profiles/sibfu/base.tex.tera',
    'profiles/sibfu/course_work.tex.tera',
    'profiles/sibfu/profile.toml',
    'examples/input.md',
]

out = open('joined.txt', 'w', encoding='utf-8')
ret = ''

for fn in files:
    ret += 'FILE: ' + fn + '\n'
    for line in open(os.path.join(cwd, fn), encoding='utf-8').read().split('\n'):
        ret += line + '\n'

out.write(ret)
