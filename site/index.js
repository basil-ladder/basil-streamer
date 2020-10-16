import { html, render } from 'lit-html';

const context = {

};

const page = () => html`
<div>
    <h1>BASIL-Ladder</h1>
</div>
${context.next5Games && context.next5Games.length > 0 ? html`
<div>
    <h3>Next ${context.next5Games.length} games:</h3>
    <ul>
        ${context.next5Games.map((item) => html`<li class=${item===context.currentGame ? "active" : null}>${item}</li>
        `)}
    </ul>
</div>` : html``}
`;

render(page(), document.body);

function connect() {
    const ws = new WebSocket("ws://localhost:9001");
    ws.onerror = (_) => {
        setTimeout(connect, 5000);
    };
    ws.onclose = (_) => {
        setTimeout(connect, 5000);
    }
    ws.onmessage = (evt) => {
        var msg = JSON.parse(evt.data);
        console.log("Received " + msg);
        const next5Games = msg['Next5Games'];
        if (next5Games) {
            context.next5Games = next5Games;
        }
        const startedReplay = msg['StartedReplay'];
        if (startedReplay) {
            context.currentGame = startedReplay;
        }
        if (msg['GameCompleted']) {
            context.currentGame = null;
        }
        render(page(), document.body);
    };
}

connect();