#!/usr/bin/env python3
"""Generate app/ui/i18n/*.json from the strings of public Android RC6 (git tag, never a working tree), plus
the few desktop-only strings below. Usage: gen_i18n.py <android repo> <out dir>"""
import json, re, subprocess, sys
import xml.etree.ElementTree as ET

TAG = 'marz-android-1.0.0-rc6-checkout-1000008'
LOCALES = {'en': 'values', 'ru': 'values-ru', 'fa': 'values-fa', 'zh-CN': 'values-zh-rCN'}
DESKTOP = {
    'desktop_helper_missing': ('The MARZ VPN system service is not running. Reinstall the app or start the MARZ VPN helper service.',
                               'Системная служба MARZ VPN не запущена. Переустановите приложение или запустите службу MARZ VPN helper.',
                               'سرویس سیستمی MARZ VPN در حال اجرا نیست. برنامه را دوباره نصب کنید یا سرویس «marzvpn-helper» را اجرا کنید.',
                               'MARZ VPN 系统服务未运行。请重新安装应用或启动“marzvpn-helper”服务。'),
    'desktop_storage_unavailable': ('The system keyring is not available, so the app cannot keep you signed in safely. Unlock the keyring or sign in to the desktop session again.',
                                    'Системное хранилище ключей недоступно, поэтому приложение не может безопасно сохранить вход. Разблокируйте хранилище или войдите в сеанс заново.',
                                    'کلیددان سیستم در دسترس نیست و برنامه نمی‌تواند ورود شما را ایمن نگه دارد. کلیددان را باز کنید یا دوباره وارد نشست شوید.',
                                    '系统密钥环不可用，应用无法安全地保存登录状态。请解锁密钥环或重新登录桌面会话。'),
    'desktop_reconnecting': ('VPN is on. Reconnecting…', 'VPN включён. Восстанавливаем соединение…', 'VPN روشن است. در حال اتصال دوباره…', 'VPN 已开启，正在重新连接…'),
    'desktop_engine_error': ('The VPN engine could not start (%1$s).', 'Не удалось запустить движок VPN (%1$s).', 'موتور VPN راه‌اندازی نشد (%1$s).', 'VPN 引擎无法启动（%1$s）。'),
    'desktop_vpn_stays_on': ('Closing this window keeps the app in the tray, and the VPN stays on, also after Quit from the tray menu, until you turn it off with the button.',
                             'При закрытии окна приложение остаётся в трее, а VPN продолжает работать, в том числе после выхода из меню трея, пока вы не выключите его кнопкой.',
                             'با بستن این پنجره برنامه در سینی سیستم می‌ماند و VPN، حتی پس از خروج از منوی سینی، روشن می‌ماند تا با دکمه خاموشش کنید.',
                             '关闭此窗口后应用保留在托盘中，VPN 保持开启，即使从托盘菜单退出也是如此，直到您用按钮关闭。'),
    # RC6 strings that name a phone, in desktop words.
    'pay_err_clock': ('Check the date and time on this computer (set them automatically) and try again.',
                      'Проверьте дату и время на этом компьютере (включите автоматическую установку) и повторите попытку.',
                      'تاریخ و ساعت این رایانه را بررسی کنید (روی خودکار بگذارید) و دوباره تلاش کنید.',
                      '请检查这台电脑的日期和时间（设为自动），然后重试。'),
    'language_system': ('System language', 'Язык системы', 'زبان سیستم', '系统语言'),
    'desktop_ks_title': ('Kill Switch', 'Kill Switch', 'Kill Switch', 'Kill Switch'),
    'desktop_ks_on_text': ('If the VPN drops, the internet is blocked until it reconnects. Nothing goes around the VPN, DNS included.',
                           'Если VPN оборвётся, интернет будет заблокирован, пока VPN не подключится снова. В обход VPN ничего не идёт, включая DNS.',
                           'اگر VPN قطع شود، اینترنت تا اتصال دوباره مسدود می‌شود. هیچ چیزی، از جمله DNS، از بیرون VPN عبور نمی‌کند.',
                           '如果 VPN 断开，互联网将被阻止，直到重新连接。任何流量（包括 DNS）都不会绕过 VPN。'),
    'desktop_ks_off_warning': ('Kill Switch is off. If the VPN drops, your traffic can go directly, without the VPN.',
                               'Kill Switch выключен. При обрыве VPN трафик может пойти напрямую, без VPN.',
                               'Kill Switch خاموش است. اگر VPN قطع شود، ترافیک شما ممکن است مستقیم و بدون VPN برود.',
                               'Kill Switch 已关闭。如果 VPN 断开，您的流量可能不经 VPN 直接发送。'),
    'desktop_ks_locked': ('Can be changed while the VPN is off.', 'Можно изменить, когда VPN выключен.', 'وقتی VPN خاموش است می‌توان آن را تغییر داد.', '可在 VPN 关闭时更改。'),
    'desktop_blocked': ('Protection is on: the internet is blocked until the VPN connects. Nothing goes around it.',
                        'Защита включена: интернет заблокирован, пока VPN не подключится. В обход VPN ничего не идёт.',
                        'محافظت روشن است: تا اتصال VPN اینترنت مسدود است و هیچ چیزی از بیرون VPN عبور نمی‌کند.',
                        '保护已开启：在 VPN 连接前互联网被阻止，任何流量都不会绕过 VPN。'),
    'desktop_other_user': ('Another user of this computer turned the VPN on. Only that user or an administrator can change it.',
                           'VPN включил другой пользователь этого компьютера. Изменить это может только этот пользователь или администратор.',
                           'کاربر دیگری از این رایانه VPN را روشن کرده است. فقط همان کاربر یا مدیر سیستم می‌تواند آن را تغییر دهد.',
                           '这台电脑的另一位用户开启了 VPN。只有该用户或管理员可以更改。'),
    'desktop_emergency_title': ('Emergency: turn off VPN and restore the internet', 'Аварийно отключить VPN и восстановить интернет', 'خاموش کردن اضطراری VPN و بازیابی اینترنت', '紧急关闭 VPN 并恢复互联网'),
    'desktop_emergency_text': ('Only if the big button did not bring your internet back. The VPN and its protection will be turned off, and your traffic will go directly, without the VPN. May ask for an administrator password.',
                               'Только если большая кнопка не вернула интернет. VPN и защита будут выключены, и трафик пойдёт напрямую, без VPN. Может потребоваться пароль администратора.',
                               'فقط اگر دکمهٔ بزرگ اینترنت را برنگرداند. VPN و محافظت آن خاموش می‌شوند و ترافیک شما مستقیم و بدون VPN می‌رود. ممکن است گذرواژهٔ مدیر لازم باشد.',
                               '仅在大按钮未能恢复互联网时使用。VPN 及其保护将被关闭，您的流量将不经 VPN 直接发送。可能需要管理员密码。'),
    'desktop_emergency_confirm': ('Turn off and go without VPN', 'Отключить и работать без VPN', 'خاموش کن و بدون VPN ادامه بده', '关闭并不使用 VPN'),
    'desktop_emergency_failed': ('It did not work. In a terminal run: sudo /usr/lib/marzvpn/marzvpn-helper --recover',
                                 'Не получилось. В терминале выполните: sudo /usr/lib/marzvpn/marzvpn-helper --recover',
                                 'انجام نشد. در ترمینال اجرا کنید: sudo /usr/lib/marzvpn/marzvpn-helper --recover',
                                 '操作失败。请在终端运行：sudo /usr/lib/marzvpn/marzvpn-helper --recover'),
    'desktop_emergency_failed_windows': ('It did not work, or the administrator prompt was refused. In a Command Prompt run as administrator: "C:\\Program Files\\MARZ VPN\\marzvpn-helper.exe" --recover',
                                         'Не получилось, либо запрос администратора отклонён. В командной строке от имени администратора выполните: "C:\\Program Files\\MARZ VPN\\marzvpn-helper.exe" --recover',
                                         'انجام نشد یا درخواست مدیر رد شد. در Command Prompt با دسترسی مدیر اجرا کنید: "C:\\Program Files\\MARZ VPN\\marzvpn-helper.exe" --recover',
                                         '操作失败，或管理员提示被拒绝。请以管理员身份打开命令提示符并运行："C:\\Program Files\\MARZ VPN\\marzvpn-helper.exe" --recover'),
    'desktop_protection_lost': ('The protection was removed from outside the app and could not be restored. Your traffic may not be protected.',
                                'Защита снята извне и не восстановилась. Трафик может быть не защищён.',
                                'محافظت از بیرون برنامه برداشته شد و بازیابی نشد. ممکن است ترافیک شما محافظت نشود.',
                                '保护被应用外部移除且无法恢复。您的流量可能未受保护。'),
    'desktop_protection_failed': ('The VPN did not start, because the protection could not be turned on. Your internet is unchanged and is NOT protected.',
                                  'VPN не запущен: защиту не удалось включить. Интернет работает как раньше и НЕ защищён.',
                                  'VPN راه‌اندازی نشد، چون محافظت فعال نشد. اینترنت شما تغییری نکرده و محافظت نمی‌شود.',
                                  'VPN 未启动，因为无法开启保护。您的互联网未改变，且未受保护。'),
    'desktop_no_address': ('The server address could not be found. Nothing was opened: check the network or turn the VPN off and on again.',
                           'Не удалось определить адрес сервера. Обхода VPN нет: проверьте сеть или выключите и снова включите VPN.',
                           'نشانی سرور پیدا نشد. هیچ راهی باز نشد: شبکه را بررسی کنید یا VPN را خاموش و دوباره روشن کنید.',
                           '找不到服务器地址。没有放行任何流量：请检查网络，或关闭后重新开启 VPN。'),
    'desktop_preview_build': ('Test build. Not for distribution.', 'Тестовая сборка. Не для распространения.', 'نسخهٔ آزمایشی. برای توزیع نیست.', '测试版本，请勿分发。'),
}

repo, out = sys.argv[1], sys.argv[2]
for index, (locale, folder) in enumerate(LOCALES.items()):
    xml = subprocess.run(['git', '-C', repo, 'show', f'{TAG}:app/src/main/res/{folder}/marz_strings.xml'], check=True, capture_output=True).stdout
    table = {}
    for s in ET.fromstring(xml).findall('string'):
        name = s.get('name')
        if name.startswith('marz_'):
            text = ''.join(s.itertext())
            table[name[5:]] = text.replace("\\'", "'").replace('\\"', '"').replace('\\n', '\n').replace('\\@', '@')
    for name, values in DESKTOP.items():
        table[name] = values[index]
    with open(f'{out}/{locale}.json', 'w', encoding='utf-8') as f:
        json.dump(table, f, ensure_ascii=False, indent=0, sort_keys=True)
        f.write('\n')
