#!/usr/bin/env python3
"""linkedin-standin.py — build the stand-in LinkedIn site linkedin-profile-walk.py serves in a guest.

  linkedin-standin.py --profile SAVED_PROFILE.html --slug SLUG \\
      [--about KEY=VANITY=SAVED_ABOUT.html ...] --out DIR

The guest has no LinkedIn sign-in and nobody is asked for one, so the walk serves saved LinkedIn
pages from 127.0.0.1 to a browser holding a cookie (the server is browser-page-guest.py
serve-site). This turns saved pages (a SavePage-WE save of a profile, and of company About tabs)
into that site, and adds what the proof needs on top:

  - every https://www.linkedin.com/ link becomes a path on the stand-in host, so the profile's
    Experience links lead to the stand-in company pages;
  - a collapsed section on every page, its text hidden (display:none, so Accessibility cannot read
    it) behind a "see more"-style button: the text comes back only if the tool presses it;
  - on the profile, a "Show all N experiences" link to its own /details/experience/ page (whose
    extra position is on that page only) and a "Show all N recommendations" link the tool must
    never open;
  - site.json, the manifest the server routes by (KEY is the profile's /company/KEY/ link, often
    the numeric id; VANITY is the name LinkedIn redirects it to).

Saved pages are real people's information: they are read from where they are and written only to
DIR, which belongs in a scratch directory or the guest, never in this repository.
"""
import argparse
import html
import html.parser
import json
from pathlib import Path
import re

SCRIPT = ('<script>document.addEventListener("click",function(e){var b=e.target.closest("[data-rx-more]");'
          'if(!b)return;var t=document.getElementById(b.getAttribute("data-rx-more"));'
          'if(t){t.style.display="inline";}b.remove();});</script>')


def marker(name):
    return 'RX-' + re.sub(r'[^A-Z0-9]+', '-', name.upper()).strip('-')


def collapsed(ident, visible, hidden, button):
    """A section whose second sentence is hidden until its button is pressed."""
    return ('<section><h2>Stand-in collapsed section</h2><p>%s <span id="%s" style="display:none">%s</span></p>'
            '<button type="button" data-rx-more="%s">%s</button></section>') % (
        html.escape(visible), ident, html.escape(hidden), ident, html.escape(button))


def local_links(text):
    return text.replace('https://www.linkedin.com/', '/')


def inject_in_main(text, block):
    """`block` and the script that opens it as the first thing in the page's <main>, so it is the
    page's content. Not before </body>: a saved page holds frames' markup in attributes, and the
    first literal </body> can be inside one of them (measured: the script never ran there)."""
    m = re.search(r'<main\b[^>]*>', text)
    if not m:
        raise SystemExit('no <main> element in a saved page; cannot place the stand-in section')
    return text[:m.end()] + block + SCRIPT + text[m.end():]


class ExperienceLinks(html.parser.HTMLParser):
    """The /company/ links under the profile's "Experience" heading, in order."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.heading, self.buf, self.in_h, self.skip, self.links = '', '', False, 0, []

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag in ('script', 'style', 'template'):
            self.skip += 1
        if tag == 'h2' and not self.skip:
            self.in_h, self.buf = True, ''
        if tag == 'a' and not self.skip and self.heading == 'experience' and '/company/' in (a.get('href') or ''):
            href = a['href'].split('?')[0]
            if href not in self.links:
                self.links.append(href)

    def handle_endtag(self, tag):
        if tag in ('script', 'style', 'template') and self.skip:
            self.skip -= 1
        if tag == 'h2' and self.in_h:
            self.in_h, self.heading = False, ' '.join(self.buf.split()).lower()

    def handle_data(self, data):
        if self.in_h and not self.skip:
            self.buf += data


def page(title, body):
    return '<!doctype html><html><head><meta charset=utf-8><title>%s</title></head><body><main>%s</main>%s</body></html>' % (
        html.escape(title), body, SCRIPT)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--profile', type=Path, required=True)
    p.add_argument('--slug', required=True)
    p.add_argument('--about', action='append', default=[], metavar='KEY=VANITY=PATH')
    p.add_argument('--out', type=Path, required=True)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    expect = {'profile_hidden': marker('profile hidden 7q2'), 'details_only': marker('details only 4k8'),
              'details_hidden': marker('details hidden 9m1'), 'companies': {}}

    raw = a.profile.read_text(encoding='utf-8', errors='ignore')
    parser = ExperienceLinks()
    parser.feed(raw)
    experience = parser.links
    if not experience:
        raise SystemExit('the saved profile has no /company/ link under an "Experience" heading')
    block = collapsed('rx-profile', 'Visible part of a collapsed profile section.',
                      'Hidden profile text %s came back after see more.' % expect['profile_hidden'], '…see more')
    block += ('<p><a href="/in/%s/details/experience/">Show all %d experiences</a> '
              '<a href="/in/%s/details/recommendations/">Show all 3 recommendations</a></p>') % (
        a.slug, len(experience) + 1, a.slug)
    profile = inject_in_main(local_links(raw), block)
    (a.out / 'profile.html').write_text(profile, encoding='utf-8')

    rows = ''.join('<li><a href="%s">Company %s</a></li>' % (html.escape(local_links(h)), html.escape(h.rstrip('/').split('/')[-1]))
                   for h in experience)
    details = ('<h2>Experience</h2><ul>%s<li><p>Stand-in position only on the details page %s.</p></li></ul>' % (
        rows, expect['details_only'])
        + collapsed('rx-details', 'Visible part of a position description.',
                    'Hidden position text %s came back after show more.' % expect['details_hidden'], 'Show more'))
    (a.out / 'details-experience.html').write_text(page('Experience', details), encoding='utf-8')

    about = {}
    for spec in a.about:
        key, vanity, path = spec.split('=', 2)
        hidden = marker('company %s hidden' % vanity)
        text = local_links(Path(path).read_text(encoding='utf-8', errors='ignore'))
        text = inject_in_main(text, collapsed('rx-company', 'Visible part of a company overview.',
                                              'Hidden company text %s came back after see more.' % hidden, 'see more'))
        name = 'company-%s.html' % vanity
        (a.out / name).write_text(text, encoding='utf-8')
        about[key] = {'vanity': vanity, 'file': name}
        expect['companies'][vanity] = hidden
    for href in experience:
        key = href.rstrip('/').split('/')[-1]
        if key not in about:
            expect['companies'][key] = marker('company %s hidden' % key)

    manifest = {'slug': a.slug, 'about': about, 'experience_keys': [h.rstrip('/').split('/')[-1] for h in experience],
                'expect': expect}
    (a.out / 'site.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'out': str(a.out), 'experience_keys': manifest['experience_keys'], 'about': list(about)}))


if __name__ == '__main__':
    main()
