import { html, render } from 'lit-html';

const context = {

};

const player_part = (p) => html`<span class="race_${p.race}">${p.name}</span>`;

const game_item = (g) => html`
<li class=${g === context.currentGame ? "active" : null}>${player_part(g.players[0])} vs ${player_part(g.players[1])} on
    ${g.map}</li>
`;

const page = () => html`
<div>
    <h1>BASIL-Ladder</h1>
</div>
${context.next5Games && context.next5Games.length > 0 ? html`
<div>
    <h3>Next ${context.next5Games.length} games:</h3>
    <ul>
        ${context.next5Games.map((item) => game_item(item))}
    </ul>
</div>` : html``}
`;

render(page(), document.body);

function connect() {
    const ws = new WebSocket("ws://localhost:" + location.port + "/service");
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