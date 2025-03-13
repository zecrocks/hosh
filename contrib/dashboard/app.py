import dash
import dash_bootstrap_components as dbc
from dash import html, dcc

# Initialize the app
app = dash.Dash(__name__, 
                external_stylesheets=[dbc.themes.BOOTSTRAP],
                suppress_callback_exceptions=True)

# Create the navbar
navbar = dbc.NavbarSimple(
    children=[
        dbc.NavItem(dbc.NavLink("Light Nodes", href="/", id="server-status-link")),
        dbc.NavItem(dbc.NavLink("Explorer Heights", href="#", id="blockchain-heights-link")),
        dbc.NavItem(dbc.NavLink("ClickHouse Data", href="#", id="clickhouse-data-link")),
    ],
    brand="Hosh Dashboard",
    brand_href="/",
    color="primary",
    dark=True,
    className="mb-4"
) 