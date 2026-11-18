import os

cwd = os.path.dirname(os.path.dirname(__file__))

files = [
    'Cargo.toml',
    'src/config/env.rs',
    'src/config/frontmatter.rs',
    'src/config/merge.rs',
    'src/config/mod.rs',
    'src/config/model.rs',
    'src/config/profile.rs',
    'src/config/yaml.rs',
    'src/images/cache.rs',
    'src/images/convert.rs',
    'src/images/mod.rs',
    'src/markdown/code.rs',
    'src/markdown/footnotes.rs',
    'src/markdown/headings.rs',
    'src/markdown/images.rs',
    'src/markdown/inline.rs',
    'src/markdown/lists.rs',
    'src/markdown/mod.rs',
    'src/markdown/plantuml.rs',
    'src/markdown/state.rs',
    'src/markdown/tables.rs',
    'src/markdown/util.rs',
    'src/template/context.rs',
    'src/template/mod.rs',
    'src/build.rs',
    'src/compile.rs',
    'src/log.rs',
    'src/main.rs',
    'src/plantuml.rs',
    'profiles/sibfu/base.tex.tera',
    'profiles/sibfu/course_work.tex.tera',
    'profiles/sibfu/practice_work.tex.tera',
    'profiles/sibfu/profile.toml',
    'examples/input.md',
    'examples/practice.md',
]

out = open('joined.txt', 'w', encoding='utf-8')
ret = ''

for fn in files:
    ret += 'FILE: ' + fn + '\n'
    for line in open(os.path.join(cwd, fn), encoding='utf-8').read().split('\n'):
        ret += line + '\n'

out.write(ret)
