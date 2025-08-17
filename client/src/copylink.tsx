import { Fragment } from 'preact';
import { useState } from 'preact/hooks';


export function CopyLink() {
    const initial_text = "<-- Click to copy!";
    const [infoText, setText] = useState(initial_text);
    const doCopy = () => {
        navigator.clipboard.writeText(window.location.href);
        setText("<-- Copied!");
        setTimeout(() => setText(initial_text), 1000);
    };
    return <Fragment>
        <span
            class="copyurl"
            onClick={e => doCopy()}
            onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { doCopy(); } }}
            role="button"
            tabIndex={0}
        >
            {window.location.href}
        </span> {infoText}
    </Fragment>;
}
