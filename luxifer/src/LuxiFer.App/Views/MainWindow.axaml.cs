using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using LuxiFer.App.Controls;
using LuxiFer.App.ViewModels;

namespace LuxiFer.App.Views;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();

        Canvas.PointerMillimeterMoved += (_, mm) => ViewModel?.ReportCursor(mm.X, mm.Y);
        Canvas.DocumentChanged += (_, _) => ViewModel?.MarkDirty();
        Canvas.ViewChanged += (_, _) => SyncRulers();
        Canvas.SelectionChanged += (_, sel) =>
        {
            if (ViewModel is not { } vm) return;
            vm.SelectedObjects.Clear();
            foreach (var o in sel) vm.SelectedObjects.Add(o);
            vm.OnSelectionChanged();
        };

        DataContextChanged += (_, _) =>
        {
            if (ViewModel is { } vm)
                vm.CanvasInvalidateRequested += (_, _) => Canvas.InvalidateVisual();
        };

        // Bett beim Einpassen freihalten: links die schwebende Werkzeug-Palette
        // (~64px), rechts das Layer-Panel (~294px), oben/unten etwas Luft.
        // Die Lineale sitzen bereits außerhalb der Canvas-Fläche.
        Canvas.ContentInset = new Thickness(64, 16, 292, 16);

        WireLayerPanel(LayerPanelRight);
        WireLayerPanel(LayerPanelLeft);

        KeyDown += OnWindowKeyDown;
        Loaded += (_, _) => SyncRulers();
    }

    private MainWindowViewModel? ViewModel => DataContext as MainWindowViewModel;

    /// <summary>Überträgt Zoom und Nullpunkt-Versatz des Canvas auf die Lineale.</summary>
    private void SyncRulers()
    {
        RulerTop.ZoomPxPerMm = Canvas.ZoomPxPerMm;
        RulerTop.OriginOffset = Canvas.PanOffset.X;
        RulerLeft.ZoomPxPerMm = Canvas.ZoomPxPerMm;
        RulerLeft.OriginOffset = Canvas.PanOffset.Y;
    }

    private void OnWindowKeyDown(object? sender, KeyEventArgs e)
    {
        if (ViewModel is null) return;

        // Undo/Redo mit Strg (auch bei fokussiertem Textfeld erlaubt)
        if (e.KeyModifiers == KeyModifiers.Control)
        {
            switch (e.Key)
            {
                case Key.Z:
                    ViewModel.UndoActionCommand.Execute(null);
                    e.Handled = true;
                    return;
                case Key.Y:
                    ViewModel.RedoActionCommand.Execute(null);
                    e.Handled = true;
                    return;
                case Key.A when ViewModel.IsDesignMode:
                    Canvas.SelectAll();
                    e.Handled = true;
                    return;
            }
            return;
        }

        // Werkzeug-Kürzel nicht auslösen, während in einem Textfeld getippt wird
        if (e.KeyModifiers != KeyModifiers.None) return;
        if (FocusManager?.GetFocusedElement() is TextBox) return;

        var tool = e.Key switch
        {
            Key.V => CanvasTool.Select,
            Key.R => CanvasTool.Rectangle,
            Key.E => CanvasTool.Ellipse,
            Key.L => CanvasTool.Line,
            Key.P => CanvasTool.Polyline,
            Key.G => CanvasTool.Polygon,
            _ => (CanvasTool?)null,
        };
        if (tool is { } t)
        {
            ViewModel.SelectToolCommand.Execute(t);
            e.Handled = true;
        }
    }

    private void WireLayerPanel(LuxiFer.App.Controls.LayerPanel panel)
    {
        panel.LayerEditRequested += async (_, layer) =>
        {
            await new LayerEditDialog(layer).ShowDialog(this);
            Canvas.InvalidateVisual(); // Farbe/Modus könnten sich geändert haben
        };
        panel.LayerToggled += (_, _) => Canvas.InvalidateVisual();
    }

    private void OnSelectionEditCommit(object? sender, RoutedEventArgs e) =>
        ViewModel?.CommitSelectionEdit();

    private void OnZoomToFitClick(object? sender, RoutedEventArgs e) =>
        Canvas.ZoomToFit();

    private void OnExitClick(object? sender, RoutedEventArgs e) => Close();
}
