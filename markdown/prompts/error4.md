PID: 599676 (WebKitWebProces)

TID: 599676 (WebKitWebProces)

UID: 1005 (daniel)

GID: 1005 (daniel)

Signal: 11 (SEGV) si_code: SEGV_MAPERR

Timestamp: Fri 2026-09-18 12:37:43 CEST (11s ago)

Command Line: /usr/lib/webkit2gtk-4.1/WebKitWebProcess 4 24

Executable: /usr/lib/webkit2gtk-4.1/WebKitWebProcess

Control Group: /user.slice/user-1005.slice/user@1005.service/app.slice/app-org.kde.konsole-42379.scope/tab(42387).scope

Unit: user@1005.service

User Unit: app-org.kde.konsole-42379.scope

Slice: user-1005.slice

Owner UID: 1005 (daniel)

Boot ID: 55c614295615425992605f857c694df2

Machine ID: 4acf8527aec94b2fa660ab17b964782d

Hostname: archlinux

Storage: /var/lib/systemd/coredump/core.WebKitWebProces.1005.55c614295615425992605f857c694df2.599676.1789727863000000.zst (present)

Size on Disk: 10.4M

Message: Process 599676 (WebKitWebProces) of user 1005 dumped core.

Stack trace of thread 599676:

#0 0x00007f134a3c984e n/a (libnvidia-eglcore.so.615.71.09 + 0x9c984e)

#1 0x00007f134a0fd471 n/a (libnvidia-eglcore.so.615.71.09 + 0x6fd471)

#2 0x00007f134a131fe6 n/a (libnvidia-eglcore.so.615.71.09 + 0x731fe6)

#3 0x00007f134a13206a n/a (libnvidia-eglcore.so.615.71.09 + 0x73206a)

#4 0x00007f134a1332ff n/a (libnvidia-eglcore.so.615.71.09 + 0x7332ff)

#5 0x00007f134a103668 n/a (libnvidia-eglcore.so.615.71.09 + 0x703668)

#6 0x00007f134a0f0981 n/a (libnvidia-eglcore.so.615.71.09 + 0x6f0981)

#7 0x00007f134a3b2238 n/a (libnvidia-eglcore.so.615.71.09 + 0x9b2238)

#8 0x00007f134a38f6cd n/a (libnvidia-eglcore.so.615.71.09 + 0x98f6cd)

#9 0x00007f13b2714e9d n/a (libEGL_nvidia.so.0 + 0x40e9d)

#10 0x00007f13b2714f72 n/a (libEGL_nvidia.so.0 + 0x40f72)

#11 0x00007f13b2716155 n/a (libEGL_nvidia.so.0 + 0x42155)

#12 0x00007f13b271c1e6 n/a (libEGL_nvidia.so.0 + 0x481e6)

#13 0x00007f13d1a64487 n/a (libwebkit2gtk-4.1.so.0 + 0x4464487)

#14 0x00007f13cf6d83e6 n/a (libwebkit2gtk-4.1.so.0 + 0x20d83e6)

#15 0x00007f13cf064514 n/a (libwebkit2gtk-4.1.so.0 + 0x1a64514)

#16 0x00007f13cf68b966 n/a (libwebkit2gtk-4.1.so.0 + 0x208b966)

#17 0x00007f13cf6d8309 n/a (libwebkit2gtk-4.1.so.0 + 0x20d8309)

#18 0x00007f13cf058fbc n/a (libwebkit2gtk-4.1.so.0 + 0x1a58fbc)

#19 0x00007f13cc2e55ff n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e55ff)

#20 0x00007f13cc2e587f n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e587f)

#21 0x00007f13c99b5c76 n/a (libglib-2.0.so.0 + 0x61c76)

#22 0x00007f13c99b5f68 g_main_context_dispatch (libglib-2.0.so.0 + 0x61f68)

#23 0x00007f13cc2e66f7 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66f7)

#24 0x00007f13cc2e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#25 0x00007f13cf6d92a8 _ZN6WebKit14WebProcessMainEiPPc (libwebkit2gtk-4.1.so.0 + 0x20d92a8)

#26 0x00007f13cd227781 n/a (libc.so.6 + 0x27781)

#27 0x00007f13cd2278b9 __libc_start_main (libc.so.6 + 0x278b9)

#28 0x0000556c9882b635 n/a (WebKitWebProcess + 0x1635)

Stack trace of thread 599678:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc34b122 n/a (libjavascriptcoregtk-4.1.so.0 + 0x234b122)

#4 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#5 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599701:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd311365 __poll (libc.so.6 + 0x111365)

#2 0x00007f13cc2e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007f13cc2e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599681:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd311365 __poll (libc.so.6 + 0x111365)

#2 0x00007f13cc2e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007f13cc2e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599682:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd311365 __poll (libc.so.6 + 0x111365)

#2 0x00007f13cc2e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007f13cc2e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599686:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd311365 __poll (libc.so.6 + 0x111365)

#2 0x00007f13cc2e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007f13cc2e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599702:

#0 0x00007f13cd31e4ed syscall (libc.so.6 + 0x11e4ed)

#1 0x00007f13c99e515e g_cond_wait (libglib-2.0.so.0 + 0x9115e)

#2 0x00007f13c997a52d n/a (libglib-2.0.so.0 + 0x2652d)

#3 0x00007f13c99ef017 n/a (libglib-2.0.so.0 + 0x9b017)

#4 0x00007f13c99eeb14 n/a (libglib-2.0.so.0 + 0x9ab14)

#5 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599680:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd311365 __poll (libc.so.6 + 0x111365)

#2 0x00007f13cc2e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007f13cc2e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599775:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f134a37b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007f134a3dfbcb n/a (libnvidia-eglcore.so.615.71.09 + 0x9dfbcb)

#5 0x00007f134a23e7e8 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7e8)

#6 0x00007f134a37edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599776:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f134a37b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007f134a23daf1 n/a (libnvidia-eglcore.so.615.71.09 + 0x83daf1)

#5 0x00007f134a23e7c0 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7c0)

#6 0x00007f134a37edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608002:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 607999:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599772:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f134a37b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007f134a23daf1 n/a (libnvidia-eglcore.so.615.71.09 + 0x83daf1)

#5 0x00007f134a23e7c0 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7c0)

#6 0x00007f134a37edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599703:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd311a57 ppoll (libc.so.6 + 0x111a57)

#2 0x00007f13c99b7f59 n/a (libglib-2.0.so.0 + 0x63f59)

#3 0x00007f13c99b8055 g_main_context_iteration (libglib-2.0.so.0 + 0x64055)

#4 0x00007f13c99b80a2 n/a (libglib-2.0.so.0 + 0x640a2)

#5 0x00007f13c99eeb14 n/a (libglib-2.0.so.0 + 0x9ab14)

#6 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#7 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 607998:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599779:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f134a37b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007f134a3dfbcb n/a (libnvidia-eglcore.so.615.71.09 + 0x9dfbcb)

#5 0x00007f134a23e7e8 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7e8)

#6 0x00007f134a37edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608049:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd311365 __poll (libc.so.6 + 0x111365)

#2 0x00007f13cc2e66c9 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22e66c9)

#3 0x00007f13cc2e6e7a _ZN3WTF7RunLoop3runEv (libjavascriptcoregtk-4.1.so.0 + 0x22e6e7a)

#4 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#5 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#6 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608003:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599773:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f134a37b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007f134a23daf1 n/a (libnvidia-eglcore.so.615.71.09 + 0x83daf1)

#5 0x00007f134a23e7c0 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7c0)

#6 0x00007f134a37edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599709:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd311a57 ppoll (libc.so.6 + 0x111a57)

#2 0x00007f13c99b7f59 n/a (libglib-2.0.so.0 + 0x63f59)

#3 0x00007f13c99b8217 g_main_loop_run (libglib-2.0.so.0 + 0x64217)

#4 0x00007f13cd140ed4 n/a (libgio-2.0.so.0 + 0x11fed4)

#5 0x00007f13c99eeb14 n/a (libglib-2.0.so.0 + 0x9ab14)

#6 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#7 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 607997:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608027:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599777:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f134a37b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007f134a23daf1 n/a (libnvidia-eglcore.so.615.71.09 + 0x83daf1)

#5 0x00007f134a23e7c0 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7c0)

#6 0x00007f134a37edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599778:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f134a37b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007f134a23daf1 n/a (libnvidia-eglcore.so.615.71.09 + 0x83daf1)

#5 0x00007f134a23e7c0 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7c0)

#6 0x00007f134a37edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608025:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608024:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608028:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608000:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608001:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608043:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608042:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608034:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608029:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608044:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608035:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 608026:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f13cc2ed637 _ZN3WTF15ThreadCondition9timedWaitERNS_5MutexENS_8WallTimeE (libjavascriptcoregtk-4.1.so.0 + 0x22ed637)

#4 0x00007f13cc2061b2 _ZN3WTF10ParkingLot21parkConditionallyImplEPKvRKNS_12ScopedLambdaIFbvEEERKNS3_IFvvEEERKNS_24TimeWithDynamicClockTypeE (libjavascriptcoregtk-4.1.so.0 + 0x22061b2)

#5 0x00007f13cc1d09d4 n/a (libjavascriptcoregtk-4.1.so.0 + 0x21d09d4)

#6 0x00007f13cc2ec289 n/a (libjavascriptcoregtk-4.1.so.0 + 0x22ec289)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

Stack trace of thread 599774:

#0 0x00007f13cd2a0952 n/a (libc.so.6 + 0xa0952)

#1 0x00007f13cd294cd9 n/a (libc.so.6 + 0x94cd9)

#2 0x00007f13cd297762 pthread_cond_timedwait (libc.so.6 + 0x97762)

#3 0x00007f134a37b4d0 n/a (libnvidia-eglcore.so.615.71.09 + 0x97b4d0)

#4 0x00007f134a23daf1 n/a (libnvidia-eglcore.so.615.71.09 + 0x83daf1)

#5 0x00007f134a23e7c0 n/a (libnvidia-eglcore.so.615.71.09 + 0x83e7c0)

#6 0x00007f134a37edea n/a (libnvidia-eglcore.so.615.71.09 + 0x97edea)

#7 0x00007f13cd2980a2 n/a (libc.so.6 + 0x980a2)

#8 0x00007f13cd32080c n/a (libc.so.6 + 0x12080c)

ELF object binary architecture: AMD x86-64