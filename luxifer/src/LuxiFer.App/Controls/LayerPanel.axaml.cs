using System;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using LuxiFer.Core.Canvas;

namespace LuxiFer.App.Controls;

/// <summary>
/// Layer-Liste („Schnitte / Ebenen"). Doppelklick auf einen Layer meldet den
/// Bearbeitungswunsch, ein Umschalten von Sichtbarkeit/Sperre ein Redraw —
/// beides über Events, damit das Control nicht selbst Fenster/Canvas kennt.
/// </summary>
public partial class LayerPanel : UserControl
{
    public LayerPanel()
    {
        InitializeComponent();
    }

    /// <summary>Doppelklick auf einen Layer (Parameter bearbeiten).</summary>
    public event EventHandler<Layer>? LayerEditRequested;

    /// <summary>Sichtbarkeit oder Sperre eines Layers wurde umgeschaltet.</summary>
    public event EventHandler? LayerToggled;

    private void OnLayerDoubleTapped(object? sender, TappedEventArgs e)
    {
        if ((sender as ListBox)?.SelectedItem is Layer layer)
            LayerEditRequested?.Invoke(this, layer);
    }

    private void OnLayerToggle(object? sender, RoutedEventArgs e) =>
        LayerToggled?.Invoke(this, EventArgs.Empty);
}
