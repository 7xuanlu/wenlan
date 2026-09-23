// SPDX-License-Identifier: Apache-2.0
// Pairing page styles only. No markup, auth, or network changes.
export const pairingCSS = `:root{color-scheme:light dark;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif;background:#fcfcfd;color:#20202b;letter-spacing:0}
*{box-sizing:border-box}
[hidden]{display:none!important}
body{margin:0;background:#fcfcfd;color:#20202b;-webkit-text-size-adjust:100%}
main{display:block;max-width:560px;margin:32px auto 0;padding:0 20px 20px}
header.brand{display:flex;align-items:center;gap:10px;margin-bottom:16px}
header.brand img{width:32px;height:32px;display:block}
header.brand span{font:600 18px Georgia,"Times New Roman",serif;letter-spacing:0}
main>img[width="48"],main>img[height="48"]{width:48px;height:48px;display:block}
h1,h2{font-family:Georgia,"Times New Roman",serif;font-weight:600;line-height:1.25;margin:0 0 12px;color:#1d1f1e}
h1{font-size:26px}
h2{font:600 14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif}
p.intro{margin:0 0 16px;font-size:14px;line-height:1.5;color:#52525e;max-width:52ch}
p{font-size:15px;line-height:1.6}
p.status{display:flex;align-items:center;gap:10px;margin:0 0 20px;font-size:14px;color:#52525e}
.status-dot{flex:0 0 auto;width:8px;height:8px;border-radius:50%;background:#8a86c9}
p.status[data-state="approved"],.status-approved{color:#1e7a4c}
p.status[data-state="approved"] .status-dot{background:#1e7a4c}
p.status[data-state="unavailable"]{color:#6b6259}
p.status[data-state="unavailable"] .status-dot{background:#a8a29a}
section.pairing-step{margin:0 0 20px}
section.pairing-step h2{margin-bottom:6px}
section.pairing-step p{margin:0 0 12px;color:#4c5350;font-size:14px}
.code-row{display:flex;gap:10px;align-items:stretch;margin:0 0 8px}
textarea#pairing-code{flex:1 1 auto;min-width:0;display:block;resize:none;width:100%;height:68px;padding:10px 12px;font:13px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;overflow-wrap:anywhere;word-break:break-all;border:1px solid #dcdce3;border-radius:6px;background:#fff;color:#20202b}
button#copy-code{align-self:center;flex:0 0 auto;width:110px;min-height:44px;white-space:nowrap}
details.client-details{margin:8px 0;border-top:1px solid #e6e6ec;padding-top:4px}
details.client-details summary{cursor:pointer;min-height:44px;display:flex;align-items:center;font-size:14px;color:#4c5350}
section.permissions{margin:0 0 16px}
section.permissions>p{font-size:13px;line-height:1.6;color:#52525e;margin:10px 0 0}
section.permissions h2{margin-bottom:8px}
section.permissions ul{margin:0;padding-left:20px;font-size:14px;line-height:1.6;color:#4c5350}
dl{margin:16px 0;font-size:14px}
dl#approved-space{margin:0 0 20px}
dt{color:#6d7470;font-size:13px}
dd{margin:2px 0 12px;overflow-wrap:anywhere;word-break:break-word}
label{display:block;margin-bottom:8px;font-size:14px}
input:not([type="checkbox"]){display:block;width:100%;max-width:100%;padding:10px 12px;margin-bottom:16px;border:1px solid #d9d6ce;border-radius:6px;font-size:16px;line-height:1.4;background:#fff;color:#1d1f1e}
.consent{display:flex;gap:10px;align-items:flex-start;margin:20px 0;font-size:14px;line-height:1.5}
.consent input{flex:0 0 auto;width:20px;height:20px;margin-top:2px}
.sample-link{display:inline-flex;align-items:center;min-height:44px;margin-top:12px}
button{cursor:pointer;border:1px solid #c9c5bb;border-radius:6px;background:#fff;color:#1d1f1e;padding:9px 16px;font-size:15px;line-height:1.4;min-height:44px;touch-action:manipulation}
button.primary{background:#242332;border-color:#242332;color:#fff}
button.primary:disabled{opacity:.55;cursor:not-allowed}
button:disabled{opacity:.6}
button:focus-visible,a:focus-visible,textarea:focus-visible,input:focus-visible,summary:focus-visible{outline:3px solid #6f6ab3;outline-offset:2px;border-radius:4px}
#notice{min-height:24px;margin:8px 0 0;font-size:14px;color:#5c625f}
.actions{display:flex;flex-wrap:wrap;gap:12px;margin-top:16px}
.actions form{margin:0}
.actions button{min-height:44px}
footer{display:flex;flex-wrap:wrap;gap:8px 20px;margin-top:20px;padding:8px 0;border-top:1px solid #e6e6ec}
footer a{font-size:13px;color:#5c625f;min-height:44px;display:inline-flex;align-items:center}
a{color:#4a4585}
@media(max-width:600px){main{margin-top:24px;padding-left:16px;padding-right:16px}h1{font-size:24px}.code-row{flex-direction:column}textarea#pairing-code{height:90px}button#copy-code{align-self:flex-end;width:110px}.actions{gap:10px}.actions form:first-child{flex:1}.actions .primary{width:100%}}
@media(prefers-color-scheme:dark){:root,body{background:#18181e;color:#e9e9ef}h1,h2{color:#f1f1f5}p.intro,section.pairing-step p,section.permissions ul,section.permissions>p,p.status,details.client-details summary,#notice{color:#bcbcc9}dt,footer a{color:#aaaabb}textarea#pairing-code,input:not([type="checkbox"]){background:#22222b;border-color:#424250;color:#e9e9ef}button{background:#22222b;border-color:#505060;color:#e9e9ef}button.primary{background:#e4e1f6;border-color:#e4e1f6;color:#20202b}details.client-details,footer{border-color:#363642}a{color:#b7b0f3}p.status[data-state="approved"]{color:#81cba6}p.status[data-state="approved"] .status-dot{background:#81cba6}button:focus-visible,a:focus-visible,textarea:focus-visible,input:focus-visible,summary:focus-visible{outline-color:#b7b0f3}}
@media(prefers-reduced-motion:reduce){*,*::before,*::after{transition:none!important;animation:none!important;scroll-behavior:auto!important}}
`;
