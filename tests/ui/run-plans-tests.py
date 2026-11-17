#!/usr/bin/env python3
"""Plans and the home card with the server's catalog (Pro off, Pro on) and the new entitlements, on both engines the desktop
app runs on (WebKitGTK on Linux, Chromium as for WebView2 on Windows), in all four languages. Presses the real buttons and
checks which plan and action they send. Synthetic state only, no network.
Usage: run-plans-tests.py <app/ui dir> [webkit|chrome|both]. Exit code 1 if any check fails."""
import json, os, re, sys, urllib.parse
sys.argv[1:1] = []  # run-update-tests.py's site(), chrome() and webkit() are reused as they are
HERE = os.path.dirname(os.path.abspath(__file__))
src = open(os.path.join(HERE, 'run-update-tests.py')).read()
exec(src[:src.index('port, w = site()')])
UI = sys.argv[1]; WHICH = sys.argv[2] if len(sys.argv) > 2 else 'both'
FA = str.maketrans('0123456789', '۰۱۲۳۴۵۶۷۸۹')
DEC = {'ru': ('ГБ', 'Мбит/с'), 'fa': ('گیگابایت', 'مگابیت‌برثانیه')}
OLD = {'en': ['Unlimited (Legacy)', 'Most freedom', 'No artificial speed cap', 'No traffic limit'], 'ru': ['Максимум свободы', 'Без искусственного ограничения скорости'],
       'fa': ['بیشترین آزادی', 'بدون محدودیت مصنوعی سرعت'], 'zh-CN': ['最自由', '无人为限速']}

port, w = site(); bad = 0
def check(name, ok, info=''):
    global bad
    print(('ok   ' if ok else 'FAIL ') + name + (f'  {info}' if not ok and info else '')); bad += not ok
strip = lambda s: s.replace('⁦', '').replace('⁩', '')
try:
    for engine, run in (('webkit', webkit), ('chrome', chrome)):
        if WHICH not in (engine, 'both'): continue
        for lang in ('en', 'ru', 'fa', 'zh-CN'):
            T = json.load(open(os.path.join(UI, 'i18n', lang + '.json'), encoding='utf8'))
            n = lambda v: str(v).translate(FA) if lang == 'fa' else str(v)
            gb_u, mbps_u = DEC.get(lang, ('GB', 'Mbps'))
            gb = lambda v: f'{n(v)} {gb_u}'; mbps = f'{n(2)} {mbps_u}'
            def t(key, *a):
                s = T[key]
                for i, x in enumerate(a): s = re.sub(r'%' + str(i + 1) + r'\$[ds]', lambda _: str(x), s)
                return s
            pro6 = t('plan_pro_name_months', n(6), n(180))
            site_label = ' · ' + T['pay_open_site']
            def load(scene, press=()):
                RESULT.clear(); run(f"http://127.0.0.1:{port}/index.html?" + urllib.parse.urlencode({'s': scene, 'lang': lang, 'press': '|'.join(press)}))
                return dict(RESULT)
            def sent(r, label):
                c = r['pressed'].get(label, {}).get('calls', [])
                return [(x[1]['plan'], x[1]['action']) for x in c if x[0] == 'checkout_start']
            tag = f'{engine} {lang}'
            pro_texts = [T['plan_pro_badge'], t('plan_pro_full_speed', gb(300), n(30)), t('plan_pro_after', mbps, gb(300)), t('plan_pro_reset', n(30)),
                         pro6, t('plan_pro_packages', n(6), n(30), gb(300)), t('plan_pro_then', mbps), t('plan_pro_save', '$4', n(6), n(30))]

            # Pro off (today's production catalog): Basic alone, renewed as Basic.
            renew_basic = t('pay_renew_plan', 'Basic') + site_label
            r = load('plans_pro_off', [renew_basic])
            if not r: check(tag + ' reported', False); continue
            text = strip(r['text'])
            check(tag + ': Pro off shows Basic only', 'Basic' in text and not any(x in text for x in pro_texts) and 'Pro' not in text.replace('Profile', ''), text[:300])
            check(tag + ': Basic renews Basic', sent(r, renew_basic) == [('basic', 'renew')], sent(r, renew_basic))
            check(tag + ': Basic in binary units', t('plan_traffic', (n(100) + ' ' + ('ГиБ' if lang == 'ru' else 'GiB'))) in text)
            check(tag + ': no old plan text', not any(o in text for o in OLD[lang]) and 'Unlimited' not in text)
            check(tag + ': direction', r['dir'] == ('rtl' if lang == 'fa' else 'ltr') and not r['overflow'])

            # Pro on, a Basic customer: every Pro text from the server's numbers; Pro is a change of plan.
            ch_pro, ch_pro6 = t('pay_switch_plan', 'Pro') + site_label, t('pay_switch_plan', pro6) + site_label
            r = load('plans_pro_on', [ch_pro, ch_pro6])
            text = strip(r['text'])
            missing = [x for x in pro_texts if x not in text]
            check(tag + ': Pro on shows Basic, Pro and Pro 6 months with server numbers', not missing and '$4' in text and '$20' in text and '$2' in text, missing)
            check(tag + ': Pro 30 days sends unlimited as change_plan', sent(r, ch_pro) == [('unlimited', 'change_plan')], sent(r, ch_pro))
            check(tag + ': Pro 6 months sends unlimited_6m as change_plan', sent(r, ch_pro6) == [('unlimited_6m', 'change_plan')], sent(r, ch_pro6))
            check(tag + ': "Unlimited data" only beside the 2 Mbps condition', T['plan_pro_badge'] not in text or t('plan_pro_after', mbps, gb(300)) in text)
            check(tag + ': no GiB in a Pro card', all('GiB' not in x and 'ГиБ' not in x for x in pro_texts))
            check(tag + ': no old plan text (Pro on)', not any(o in text for o in OLD[lang]) and 'Unlimited (' not in text and not re.search(r'Unlimited(?! data)', text))
            check(tag + ': no horizontal overflow', not r['overflow'])

            # Signed out: choose, then the site checkout of exactly that plan.
            r = load('plans_out_pro_on', [t('plan_choose', pro6)])
            check(tag + ': signed out lists all three', all(t('plan_choose', x) in strip(r['text']) for x in ('Basic', 'Pro', pro6)))

            # A Pro customer renews Pro (either period) and changes to Basic.
            rn_pro, rn_pro6, ch_basic = t('pay_renew_plan', 'Pro') + site_label, t('pay_renew_plan', pro6) + site_label, t('pay_switch_plan', 'Basic') + site_label
            r = load('plans_pro_holder', [rn_pro, rn_pro6, ch_basic])
            check(tag + ': Pro customer: Pro renews', sent(r, rn_pro) == [('unlimited', 'renew')] and sent(r, rn_pro6) == [('unlimited_6m', 'renew')], (sent(r, rn_pro), sent(r, rn_pro6)))
            check(tag + ': Pro customer: Basic is a change', sent(r, ch_basic) == [('basic', 'change_plan')], sent(r, ch_basic))

            # The plan paid before Pro: renewed only as Pro, never under its old name.
            r = load('plans_unlimited_holder', [rn_pro, ch_basic])
            check(tag + ': previous plan renews as Pro', sent(r, rn_pro) == [('unlimited', 'renew')] and sent(r, ch_basic) == [('basic', 'change_plan')], (sent(r, rn_pro), sent(r, ch_basic)))
            r = load('plans_unlimited_holder_pro_off', [ch_basic])
            text = strip(r['text'])
            check(tag + ': previous plan, Pro off: Basic only, no renewal of the old plan', sent(r, ch_basic) == [('basic', 'change_plan')] and T['plan_pro_badge'] not in text and not any(o in text for o in OLD[lang]))

            # Home.
            r = load('home_pro'); text = strip(r['text'])
            check(tag + ': home Pro: package left', 'Pro' in text and t('plan_pro_left', gb(176), gb(300)) in text, text[-600:])
            r = load('home_pro_limited'); text = strip(r['text'])
            check(tag + ': home Pro limited: 2 Mbps until the next package', T['plan_pro_limited'].split('%')[0] in text and mbps in text and gb(300) in text)
            r = load('home_pro_last_package'); text = strip(r['text'])
            check(tag + ': home Pro limited in the last package', mbps in text)
            r = load('home_pro_stale'); text = strip(r['text'])
            check(tag + ': home Pro stale: checking, no numbers', T['plan_checking_details'] in text and gb(250) not in text)
            r = load('home_unlimited'); text = strip(r['text'])
            renew_btn = any(b['text'] == T['renew'] for b in r['buttons'])
            check(tag + ': previous plan on home: neutral name, no renewal offer', T['plan_your_paid'] in text and T['plan_no_limit_until'].split('%')[0].strip() in text and not renew_btn and not any(o in text for o in OLD[lang]) and 'Unlimited' not in text)
            r = load('home_unlimited_expired'); text = strip(r['text'])
            check(tag + ': previous plan ended: no renewal of it', not any(b['text'] == T['renew'] for b in r['buttons']) and not any(o in text for o in OLD[lang]))
            r = load('home_verifying'); text = strip(r['text'])
            check(tag + ': plan being verified', T['plan_paid'] in text and T['plan_checking_details'] in text)
            r = load('home_off'); text = strip(r['text'])
            check(tag + ': Basic home unchanged', 'Basic' in text and any(b['text'] == T['renew'] for b in r['buttons']))
finally:
    shutil.rmtree(w, ignore_errors=True)
print('FAILED' if bad else 'plans UI tests passed'); sys.exit(1 if bad else 0)
