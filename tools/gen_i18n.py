#!/usr/bin/env python3
"""Generate app/ui/i18n/*.json from the strings of public Android RC6 (git tag, never a working tree), plus
the desktop-only strings below (the only place to add or change them). Refuses to drop a string the JSON already has.
Usage: gen_i18n.py <android repo> <out dir> [--allow-remove]"""
import json, re, subprocess, sys
import xml.etree.ElementTree as ET

TAG = 'marz-android-1.0.0-rc6-checkout-1000008'
LOCALES = {'en': 'values', 'ru': 'values-ru', 'fa': 'values-fa', 'zh-CN': 'values-zh-rCN'}
DESKTOP = {
    'desktop_helper_missing': ('The MARZ VPN system service is not running. Reinstall the app or start the MARZ VPN helper service.',
                               'Системная служба MARZ VPN не запущена. Переустановите приложение или запустите службу MARZ VPN helper.',
                               'سرویس سیستمی MARZ VPN در حال اجرا نیست. برنامه را دوباره نصب کنید یا سرویس «marzvpn-helper» را اجرا کنید.',
                               'MARZ VPN 系统服务未运行。请重新安装应用或启动“marzvpn-helper”服务。'),
    'desktop_helper_mismatch': ('Parts of MARZ VPN are from different versions. Run the installer again: your sign-in and settings stay.',
                                'Компоненты MARZ VPN относятся к разным версиям. Запустите установщик ещё раз: вход и настройки сохранятся.',
                                'اجزای MARZ VPN از نسخه\u200cهای متفاوتی هستند. نصب\u200cکننده را دوباره اجرا کنید: ورود و تنظیمات شما حفظ می\u200cشود.',
                                'MARZ VPN 的组件版本不一致。请重新运行安装程序：您的登录状态和设置会保留。'),
    'open_source_note': ('MARZ VPN uses open-source software under GPL-3.0. The source code of every release is published.', 'MARZ VPN использует открытое ПО по лицензии GPL-3.0. Исходный код каждого выпуска опубликован.', 'MARZ VPN از نرم\u200cافزار متن\u200cباز با مجوز GPL-3.0 استفاده می\u200cکند. کد منبع هر نسخه منتشر شده است.', 'MARZ VPN 使用 GPL-3.0 许可的开源软件。每个版本的源代码均已公开。'),
    'desktop_source_title': ('Source code', 'Исходный код', 'کد منبع', '源代码'),
    'desktop_storage_unavailable': ('The system keyring is not available, so the app cannot keep you signed in safely. Unlock the keyring or sign in to the desktop session again.',
                                    'Системное хранилище ключей недоступно, поэтому приложение не может безопасно сохранить вход. Разблокируйте хранилище или войдите в сеанс заново.',
                                    'کلیددان سیستم در دسترس نیست و برنامه نمی‌تواند ورود شما را ایمن نگه دارد. کلیددان را باز کنید یا دوباره وارد نشست شوید.',
                                    '系统密钥环不可用，应用无法安全地保存登录状态。请解锁密钥环或重新登录桌面会话。'),
    'desktop_reconnecting': ('VPN is on. Reconnecting…', 'VPN включён. Восстанавливаем соединение…', 'VPN روشن است. در حال اتصال دوباره…', 'VPN 已开启，正在重新连接…'),
    'desktop_engine_error': ('The VPN engine could not start (%1$s).', 'Не удалось запустить движок VPN (%1$s).', 'موتور VPN راه‌اندازی نشد (%1$s).', 'VPN 引擎无法启动（%1$s）。'),
    'desktop_vpn_stays_on': ('Closing this window keeps the app in the tray, and the VPN stays on. Quit in the tray menu turns the VPN off and closes the app.', 'При закрытии окна приложение остаётся в трее, а VPN продолжает работать. Пункт «Выйти» в меню трея отключает VPN и закрывает приложение.', 'با بستن این پنجره برنامه در سینی سیستم می\u200cماند و VPN روشن می\u200cماند. گزینهٔ خروج در منوی سینی، VPN را خاموش می\u200cکند و برنامه را می\u200cبندد.', '关闭此窗口后应用保留在托盘中，VPN 保持开启。托盘菜单中的“退出”会关闭 VPN 并退出应用。'),
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
    'desktop_quit_back': ('Back to the app', 'Вернуться в приложение', 'بازگشت به برنامه', '返回应用'),
    'desktop_quit_confirm_button': ('Disconnect VPN and Exit', 'Отключить VPN и выйти', 'قطع VPN و خروج', '断开 VPN 并退出'),
    'desktop_quit_confirm_text': ('Exiting will disconnect the VPN. Your internet traffic will continue directly through your regular connection.', 'При выходе VPN будет отключён, а интернет-трафик продолжит передаваться напрямую через ваше обычное подключение.', 'با خروج، VPN قطع می\u200cشود و ترافیک اینترنت شما مستقیماً از طریق اتصال معمولی\u200cتان ادامه می\u200cیابد.', '退出后 VPN 将断开，您的互联网流量将继续通过常规连接直接传输。'),
    'desktop_quit_confirm_title': ('Exit MARZ VPN?', 'Выйти из MARZ VPN?', 'از MARZ VPN خارج می\u200cشوید؟', '退出 MARZ VPN？'),
    'desktop_quit_failed_text': ('The VPN could not be confirmed off, so the app stayed open. The internet may not work. Try again. If that does not help, go back to the app and open Settings, then the emergency option that turns the VPN off and restores the internet.', 'Не удалось подтвердить отключение VPN, поэтому приложение не закрыто. Интернет может не работать. Повторите попытку. Если не поможет, вернитесь в приложение и откройте «Настройки» → «Аварийно отключить VPN и восстановить интернет».', 'خاموش شدن VPN تأیید نشد، بنابراین برنامه باز ماند. ممکن است اینترنت کار نکند. دوباره تلاش کنید. اگر فایده نداشت، به برنامه برگردید و در تنظیمات گزینهٔ اضطراری خاموش کردن VPN و بازگرداندن اینترنت را باز کنید.', '无法确认 VPN 已关闭，因此应用未退出。互联网可能无法使用。请重试。如果仍无效，请返回应用并打开“设置”中用于紧急关闭 VPN 并恢复互联网的选项。'),
    'desktop_quit_failed_title': ('The VPN is not off', 'VPN не отключён', 'VPN خاموش نشد', 'VPN 未关闭'),
    'desktop_quit_progress': ('Turning the VPN off…', 'Отключаем VPN…', 'در حال خاموش کردن VPN…', '正在关闭 VPN…'),
    'desktop_quit_retry': ('Try again', 'Повторить', 'تلاش دوباره', '重试'),
    # Android strings worded for a computer instead of a phone.
    'pay_err_clock': ('Check the date and time on this computer (set them automatically) and try again.', 'Проверьте дату и время на этом компьютере (включите автоматическую установку) и повторите попытку.', 'تاریخ و ساعت این رایانه را بررسی کنید (روی خودکار بگذارید) و دوباره تلاش کنید.', '请检查这台电脑的日期和时间（设为自动），然后重试。'),
    'update_check': ('Check for updates', 'Проверить обновления', 'بررسی به\u200cروزرسانی', '检查更新'),
    'update_checking': ('Checking…', 'Проверяем…', 'در حال بررسی…', '正在检查…'),
    'update_latest': ('You have the latest version.', 'Установлена последняя версия.', 'آخرین نسخه نصب است.', '已是最新版本。'),
    'update_available': ('New version available: %1$s', 'Доступна новая версия: %1$s', 'نسخهٔ جدید در دسترس است: %1$s', '有新版本可用：%1$s'),
    'update_download': ('Download', 'Скачать', 'دانلود', '下载'),
    'update_later': ('Later', 'Позже', 'بعداً', '稍后'),
    'update_failed': ('Could not check for updates.', 'Не удалось проверить обновления.', 'بررسی به\u200cروزرسانی انجام نشد.', '无法检查更新。'),
    'update_retry': ('Try again', 'Повторить', 'تلاش دوباره', '重试'),
    'desktop_preview_build': ('Test build. Not for distribution.', 'Тестовая сборка. Не для распространения.', 'نسخهٔ آزمایشی. برای توزیع نیست.', '测试版本，请勿分发。'),
    # Pro (owner decision 2026-11-16), wording of the approved site copy site/frontend/src/data/pro.ts (the same source as
    # Android 1.0.3). Every number is the server's catalog or entitlement, formatted by the app: %s, never typed in here.
    'plan_pro_badge': ('Unlimited data', 'Безлимитный трафик', 'ترافیک نامحدود', '不限流量'),
    'plan_pro_name_months': ('Pro %1$s months (%2$s days)', 'Pro %1$s месяцев (%2$s дней)', 'Pro %1$s ماه (%2$s روز)', 'Pro %1$s 个月（%2$s 天）'),
    'plan_pro_full_speed': ('%1$s at full speed every %2$s days', '%1$s на полной скорости каждые %2$s дней', 'هر %2$s روز %1$s با سرعت کامل', '每 %2$s 天 %1$s 全速流量'),
    'plan_pro_after': ('Unlimited data at up to %1$s after %2$s', 'После %2$s — безлимитный трафик на скорости до %1$s', 'پس از %2$s، ترافیک نامحدود با سرعت حداکثر %1$s', '用完 %2$s 后不限流量，速度最高 %1$s'),
    'plan_pro_reset': ('Full speed resets with each new %1$s-day package', 'Полная скорость возвращается с каждым новым %1$s-дневным пакетом', 'با هر بسته %1$s روزه جدید، سرعت کامل برمی‌گردد', '每个新的 %1$s 天流量包自动恢复全速'),
    'plan_pro_packages': ('%1$s × %2$s-day packages, %3$s at full speed each', '%1$s × %2$s-дневных пакетов, в каждом %3$s на полной скорости', '%1$s بسته %2$s روزه، هرکدام با %3$s سرعت کامل', '%1$s 个 %2$s 天流量包，每个包含 %3$s 全速流量'),
    'plan_pro_then': ('then up to %1$s until the next package', 'затем до %1$s до начала следующего пакета', 'سپس حداکثر %1$s تا شروع بسته بعدی', '之后最高 %1$s，直至下一个流量包开始'),
    'plan_pro_save': ('Save %1$s compared with %2$s × %3$s-day payments.', 'Экономия %1$s по сравнению с %2$s оплатами по %3$s дней.', '%1$s صرفه‌جویی نسبت به %2$s پرداخت %3$s روزه.', '比分 %2$s 次每次 %3$s 天付款节省 %1$s。'),
    'plan_pro_left': ('%1$s of %2$s left in this package', 'В этом пакете осталось %1$s из %2$s', '%1$s از %2$s در این بسته باقی مانده است', '本流量包剩余 %1$s（共 %2$s）'),
    'plan_pro_limited': ('%1$s used — up to %2$s until %3$s', '%1$s израсходованы — до %3$s скорость до %2$s', '%1$s مصرف شد — تا %3$s سرعت حداکثر %2$s', '%1$s 已用完 — %3$s 前速度最高 %2$s'),
    'plan_checking_details': ('Checking plan details', 'Проверяем детали тарифа', 'در حال بررسی جزئیات طرح', '正在核实套餐详情'),
    # A plan paid before Pro: never named; it keeps its rights until its paid end.
    'plan_your_paid': ('Your paid plan', 'Ваш оплаченный тариф', 'طرح پرداخت‌شده شما', '您的付费套餐'),
    'plan_no_limit_until': ('No data limit until %1$s', 'Без ограничения трафика до %1$s', 'بدون محدودیت حجم تا %1$s', '%1$s 前不限流量'),
    'plan_paid': ('Paid plan', 'Оплаченный тариф', 'طرح پرداخت‌شده', '付费套餐'),
    # Telegram code sign-in (2026-11-18, App API docs/app/TELEGRAM-CODE-LOGIN.md): the page on marzvpn.com, the code from
    # @MarzReality_bot on the phone. Telegram Desktop is not needed. The number-matching screen keeps its Android strings.
    'login_chooser_telegram_hint': ('If you bought in our Telegram bot. The code comes to Telegram on your phone: Telegram Desktop is not needed.',
                                    'Если вы покупали в нашем Telegram-боте. Код придёт в Telegram на телефоне: Telegram Desktop не нужен.',
                                    'اگر از ربات تلگرام ما خرید کرده‌اید. کد در تلگرامِ گوشی شما می‌آید و به Telegram Desktop نیازی نیست.',
                                    '如果您在我们的 Telegram 机器人中购买。验证码会发送到您手机上的 Telegram，无需安装 Telegram Desktop。'),
    'desktop_tg_step1': ('Press the button: a sign-in page opens in your browser.', 'Нажмите кнопку — в браузере откроется страница входа.',
                         'دکمه را بزنید تا صفحهٔ ورود در مرورگر شما باز شود.', '点击按钮，浏览器中会打开登录页面。'),
    'desktop_tg_step2': ('On your phone, open @MarzReality_bot (scan the QR code on the page) and get a 6-digit code.',
                         'На телефоне откройте @MarzReality_bot (отсканируйте QR-код на странице) и получите 6-значный код.',
                         'در گوشی، \u200e@MarzReality_bot را باز کنید (کد QR روی صفحه را اسکن کنید) و یک کد ۶ رقمی بگیرید.',
                         '在手机上打开 @MarzReality_bot（扫描页面上的二维码），获取 6 位验证码。'),
    'desktop_tg_step3': ('Enter the code on the page. Sign-in in the app finishes by itself.', 'Введите код на странице. Вход в приложении завершится сам.',
                         'کد را در صفحه وارد کنید. ورود در برنامه خودبه‌خود کامل می‌شود.', '在页面上输入验证码，应用会自动完成登录。'),
    'desktop_tg_start': ('Open the sign-in page', 'Открыть страницу входа', 'باز کردن صفحهٔ ورود', '打开登录页面'),
    'desktop_tg_code_intro': ('A sign-in page opened in your browser. On your phone open @MarzReality_bot (scan the QR code on the page) and get a code. Enter it on the page.',
                              'В браузере открылась страница входа. На телефоне откройте @MarzReality_bot (отсканируйте QR-код на странице) и получите код. Введите его на странице.',
                              'صفحهٔ ورود در مرورگر شما باز شد. در گوشی \u200e@MarzReality_bot را باز کنید (کد QR روی صفحه را اسکن کنید) و کد بگیرید. کد را در همان صفحه وارد کنید.',
                              '浏览器中已打开登录页面。请在手机上打开 @MarzReality_bot（扫描页面上的二维码）获取验证码，然后在该页面上输入。'),
    'desktop_tg_ref': ('Request number: %1$s', 'Номер запроса: %1$s', 'شمارهٔ درخواست: %1$s', '请求编号：%1$s'),
    'desktop_tg_ref_hint': ('The page and the bot show the same number.', 'На странице и в боте будет тот же номер.',
                            'صفحه و ربات همین شماره را نشان می‌دهند.', '页面和机器人中会显示相同的编号。'),
    'desktop_tg_progress_waiting': ('Waiting for the sign-in page to be opened…', 'Ждём, когда откроется страница входа…',
                                    'در انتظار باز شدن صفحهٔ ورود…', '正在等待打开登录页面…'),
    'desktop_tg_progress_page_opened': ('The page is open. Get the code in @MarzReality_bot on your phone.',
                                        'Страница открыта. Получите код в @MarzReality_bot на телефоне.',
                                        'صفحه باز است. کد را در \u200e@MarzReality_bot روی گوشی بگیرید.', '页面已打开。请在手机上的 @MarzReality_bot 中获取验证码。'),
    'desktop_tg_progress_code_requested': ('The bot is sending the code to your Telegram…', 'Бот отправляет код в ваш Telegram…',
                                           'ربات در حال فرستادن کد به تلگرام شماست…', '机器人正在向您的 Telegram 发送验证码…'),
    'desktop_tg_progress_code_sent': ('The code was sent to your Telegram. Enter it on the page.', 'Код отправлен в ваш Telegram. Введите его на странице.',
                                      'کد به تلگرام شما فرستاده شد. آن را در صفحه وارد کنید.', '验证码已发送到您的 Telegram，请在页面上输入。'),
    'desktop_tg_progress_delivery_failed': ('Could not deliver the code. Open @MarzReality_bot and press Start, then try again.',
                                            'Не удалось доставить код. Откройте @MarzReality_bot, нажмите «Start» и попробуйте снова.',
                                            'کد تحویل داده نشد. \u200e@MarzReality_bot را باز کنید و «Start» را بزنید، سپس دوباره تلاش کنید.',
                                            '验证码无法送达。请打开 @MarzReality_bot 并点击“Start”，然后重试。'),
    'desktop_tg_offline': ('No connection to MARZ VPN. Still waiting: the request is kept.', 'Нет связи с MARZ VPN. Ожидание продолжается: запрос сохранён.',
                           'اتصال به MARZ VPN برقرار نیست. همچنان منتظریم و درخواست حفظ شده است.', '无法连接到 MARZ VPN。仍在等待，请求已保留。'),
    'desktop_tg_open_page': ('Open the page again', 'Открыть страницу ещё раз', 'باز کردن دوبارهٔ صفحه', '重新打开页面'),
    'desktop_tg_use_email': ('Continue with email', 'Продолжить через Email', 'ادامه با ایمیل', '改用邮箱继续'),
    'desktop_tg_enter_here': ('Enter the code here instead', 'Ввести код здесь', 'وارد کردن کد در همین‌جا', '改为在此输入验证码'),
    'desktop_tg_manual_hint': ('No page? In @MarzReality_bot on your phone send %1$s, get a code and enter it here.',
                               'Нет страницы? Отправьте %1$s в @MarzReality_bot на телефоне, получите код и введите его здесь.',
                               'صفحه باز نشد؟ در \u200e@MarzReality_bot روی گوشی %1$s را بفرستید، کد بگیرید و آن را اینجا وارد کنید.',
                               '没有页面？请在手机上的 @MarzReality_bot 中发送 %1$s，获取验证码后在此输入。'),
    'desktop_tg_code_label': ('6-digit code from the bot', '6-значный код из бота', 'کد ۶ رقمی ربات', '机器人发送的 6 位验证码'),
    'desktop_tg_err_wrong': ('Wrong code. Attempts left: %1$s.', 'Неверный код. Осталось попыток: %1$s.', 'کد اشتباه است. تلاش‌های باقی‌مانده: %1$s.', '验证码错误。剩余尝试次数：%1$s。'),
    'desktop_tg_err_format': ('Enter the 6 digits from the bot.', 'Введите 6 цифр из бота.', '۶ رقم کدی را که ربات فرستاده وارد کنید.', '请输入机器人发送的 6 位数字。'),
    'desktop_tg_err_code_expired': ('This code has expired. Ask the bot for a new code.', 'Срок действия кода истёк. Запросите в боте новый код.',
                                    'این کد منقضی شده است. از ربات کد جدید بخواهید.', '此验证码已过期，请向机器人获取新的验证码。'),
    'desktop_tg_err_not_requested': ('No code has been requested yet. Ask for one in @MarzReality_bot first.', 'Код ещё не запрошен. Сначала запросите его в @MarzReality_bot.',
                                     'هنوز کدی درخواست نشده است. ابتدا از \u200e@MarzReality_bot کد بخواهید.', '尚未申请验证码。请先在 @MarzReality_bot 中获取。'),
    'desktop_tg_err_network': ('No connection to MARZ VPN. The request is kept: try again.', 'Нет связи с MARZ VPN. Запрос сохранён — попробуйте ещё раз.',
                               'اتصال به MARZ VPN برقرار نیست. درخواست حفظ شده است؛ دوباره تلاش کنید.', '无法连接到 MARZ VPN。请求已保留，请重试。'),
    'desktop_tg_err_attempts': ('Too many wrong codes. Start again.', 'Слишком много неверных кодов. Начните заново.',
                                'کدهای اشتباه زیادی وارد شد. دوباره شروع کنید.', '错误验证码次数过多，请重新开始。'),
    'desktop_tg_err_unavailable': ('Sign-in with Telegram is unavailable right now. Try again later or sign in with email.',
                                   'Вход через Telegram сейчас недоступен. Попробуйте позже или войдите через Email.',
                                   'ورود با تلگرام در حال حاضر در دسترس نیست. بعداً دوباره تلاش کنید یا با ایمیل وارد شوید.',
                                   'Telegram 登录暂时不可用。请稍后重试，或使用邮箱登录。'),
}
# Android strings the desktop no longer ships: the card of the plan sold before Pro (owner decision 2026-11-16: its name,
# badge and "no speed cap" disappear in every language). Removing one more needs it added here.
DROP = {'plan_unlimited_badge', 'plan_no_speed_cap', 'plan_no_traffic_limit', 'plan_ip3'}

args = [a for a in sys.argv[1:] if a != '--allow-remove']
repo, out = args[0], args[1]
# Strings added by hand to the JSON and not here would vanish on the next run: refuse instead, and say which.
existing = set()
for locale in LOCALES:
    try:
        existing |= set(json.load(open(f'{out}/{locale}.json', encoding='utf-8')))
    except FileNotFoundError:
        pass
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
    for name in DROP:
        table.pop(name, None)
    lost = sorted(existing - set(table) - DROP)
    if lost and '--allow-remove' not in sys.argv:
        sys.exit(f'REFUSED: these strings exist in {out} but not in Android {TAG} or DESKTOP; add them to DESKTOP first: {lost}')
    with open(f'{out}/{locale}.json', 'w', encoding='utf-8') as f:
        json.dump(table, f, ensure_ascii=False, indent=0, sort_keys=True)
        f.write('\n')
